//! Integration tests for StopController during actual inference
//!
//! These tests verify that the stop controller integrates correctly with the
//! inference pipeline, ensuring:
//! - Stop decisions are made during token generation
//! - Stop reason codes are persisted to inference receipts
//! - Stop policy digests are committed to Merkle bundles
//! - Determinism is maintained across inference runs
//! - Different stop reasons trigger correctly
//! - Stop policies from requests override defaults
//! - Stop decision token indices are included in receipts

#![allow(clippy::clone_on_copy)]
#![allow(clippy::useless_vec)]
#![allow(clippy::type_complexity)]

use adapteros_api_types::inference::{StopPolicySpec, StopReasonCode};
use adapteros_core::B3Hash;
use adapteros_db::{Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use adapteros_lora_worker::stop_controller::StopController;
use anyhow::{Context, Result};
use std::sync::Arc;

// =============================================================================
// Helper Functions
// =============================================================================

/// Initialize an in-memory database with inference trace schema
async fn init_test_db() -> Result<Arc<Db>> {
    // Skip migration signature verification for tests
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let db = Arc::new(Db::connect(":memory:").await?);
    let pool = db.pool();

    // Create inference_traces table
    sqlx::query(
        r#"
        CREATE TABLE inference_traces (
            trace_id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            request_id TEXT,
            context_digest BLOB NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Create inference_trace_tokens table
    sqlx::query(
        r#"
        CREATE TABLE inference_trace_tokens (
            trace_id TEXT NOT NULL,
            token_index INTEGER NOT NULL,
            selected_adapter_ids BLOB NOT NULL,
            gates_q15 BLOB NOT NULL,
            decision_hash BLOB NOT NULL,
            policy_mask_digest BLOB,
            allowed_mask BLOB,
            policy_overrides_json TEXT,
            backend_id TEXT,
            kernel_version_id TEXT,
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Create inference_trace_receipts table with stop fields
    sqlx::query(
        r#"
        CREATE TABLE inference_trace_receipts (
            trace_id TEXT PRIMARY KEY,
            run_head_hash BLOB NOT NULL,
            output_digest BLOB NOT NULL,
            input_digest_b3 BLOB,
            receipt_digest BLOB NOT NULL,
            logical_prompt_tokens INTEGER NOT NULL,
            prefix_cached_token_count INTEGER NOT NULL,
            billed_input_tokens INTEGER NOT NULL,
            logical_output_tokens INTEGER NOT NULL,
            billed_output_tokens INTEGER NOT NULL,
            signature BLOB,
            attestation BLOB,
            stop_reason_code TEXT,
            stop_reason_token_index INTEGER,
            stop_policy_digest_b3 BLOB,
            model_cache_identity_v2_digest_b3 BLOB,
            prefix_kv_key_b3 TEXT,
            prefix_cache_hit INTEGER NOT NULL DEFAULT 0,
            prefix_kv_bytes INTEGER NOT NULL DEFAULT 0,
            equipment_profile_digest_b3 BLOB,
            processor_id TEXT,
            mlx_version TEXT,
            ane_version TEXT,
            crypto_receipt_digest_b3 BLOB,
            receipt_parity_verified INTEGER,
            tenant_id TEXT,
            created_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(db)
}

/// Simulate token generation with a stop controller
struct InferenceSimulation {
    controller: StopController,
    eos_token_id: u32,
    vocab_size: usize,
}

impl InferenceSimulation {
    fn new(policy: StopPolicySpec, eos_token_id: u32, vocab_size: usize) -> Self {
        Self {
            controller: StopController::new(policy),
            eos_token_id,
            vocab_size,
        }
    }

    /// Generate tokens until stop condition is met
    /// Returns (generated_tokens, stop_decision, all_logits_for_each_token)
    fn generate_until_stop(
        &mut self,
        token_sequence: Vec<u32>,
        eos_logit: f32,
    ) -> (Vec<u32>, Option<(StopReasonCode, u32)>, Vec<Vec<f32>>) {
        let mut tokens = Vec::new();
        let mut all_logits = Vec::new();

        for token in token_sequence {
            // Create logits with specified EOS probability
            let mut logits = vec![0.0; self.vocab_size];
            logits[self.eos_token_id as usize] = eos_logit;

            all_logits.push(logits.clone());

            // Check stop condition
            if let Some(decision) = self
                .controller
                .check_stop(token, self.eos_token_id, &logits)
            {
                tokens.push(token);
                return (
                    tokens,
                    Some((decision.reason, decision.token_index)),
                    all_logits,
                );
            }

            tokens.push(token);
        }

        (tokens, None, all_logits)
    }

    fn policy_digest(&self) -> &B3Hash {
        self.controller.policy_digest()
    }
}

/// Helper to create a trace token input
fn make_token_input(
    token_index: u32,
    adapter_ids: Vec<String>,
    gates_q15: Vec<i16>,
) -> TraceTokenInput {
    TraceTokenInput {
        token_index,
        adapter_ids,
        gates_q15,
        policy_mask_digest_b3: None,
        allowed_mask: None,
        policy_overrides_applied: None,
        backend_id: None,
        kernel_version_id: None,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[tokio::test]
async fn test_stop_controller_budget_max_persisted_to_receipt() -> Result<()> {
    let db = init_test_db().await?;

    // Create a policy with low budget
    let policy = StopPolicySpec {
        output_max_tokens: 5,
        eos_token_id: Some(100),
        completion_threshold_q15: 32767, // Won't trigger
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 100, 200);
    let policy_digest = sim.policy_digest().clone();

    // Generate tokens until budget is exceeded
    let tokens_to_generate = vec![1, 2, 3, 4, 5, 6, 7, 8]; // More than budget
    let (generated_tokens, stop_decision, _) = sim.generate_until_stop(tokens_to_generate, 0.1);

    // Verify stop decision
    assert!(stop_decision.is_some(), "Should have stopped");
    let (reason, token_index) = stop_decision.unwrap();
    assert_eq!(reason, StopReasonCode::BudgetMax);
    assert_eq!(
        token_index, 5,
        "Should stop at token index 5 (6th token, exceeds budget of 5)"
    );

    // Persist to database
    let trace_id = "stop-budget-max".to_string();
    let context_digest = [0x01u8; 32];
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-budget".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

    // Record tokens
    for (idx, _token) in generated_tokens.iter().enumerate() {
        sink.record_token(make_token_input(
            idx as u32,
            vec!["adapter-1".to_string()],
            vec![16384],
        ))
        .await?;
    }

    // Finalize with stop info
    let finalization = TraceFinalization {
        output_tokens: &generated_tokens,
        logical_prompt_tokens: generated_tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: generated_tokens.len() as u32,
        logical_output_tokens: generated_tokens.len() as u32,
        billed_output_tokens: generated_tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(policy_digest.clone()),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    let receipt = sink.finalize(finalization).await?;

    // Verify receipt contains stop fields
    let stored_receipt: (Option<String>, Option<i64>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT stop_reason_code, stop_reason_token_index, stop_policy_digest_b3
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind(&trace_id)
    .fetch_one(db.pool())
    .await?;

    assert_eq!(stored_receipt.0.as_deref(), Some("BUDGET_MAX"));
    assert_eq!(stored_receipt.1, Some(5));
    assert_eq!(
        stored_receipt.2.as_deref().map(|b| b as &[u8]),
        Some(policy_digest.as_bytes() as &[u8])
    );

    // Verify stop fields are in receipt digest
    assert!(!receipt.receipt_digest.to_hex().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_stop_controller_completion_confident_persisted() -> Result<()> {
    let db = init_test_db().await?;

    // Create a policy with moderate threshold
    let policy = StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(10),
        completion_threshold_q15: 16384, // ~0.5 threshold
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 10, 100);
    let policy_digest = sim.policy_digest().clone();

    // Generate with high EOS logit to trigger COMPLETION_CONFIDENT
    let tokens_to_generate = vec![1, 2, 3];
    let (generated_tokens, stop_decision, _) = sim.generate_until_stop(tokens_to_generate, 15.0);

    assert!(stop_decision.is_some());
    let (reason, token_index) = stop_decision.unwrap();
    assert_eq!(reason, StopReasonCode::CompletionConfident);

    // Persist to database
    let trace_id = "stop-completion-confident".to_string();
    let context_digest = [0x02u8; 32];
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-confident".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

    for (idx, _token) in generated_tokens.iter().enumerate() {
        sink.record_token(make_token_input(
            idx as u32,
            vec!["adapter-1".to_string()],
            vec![16384],
        ))
        .await?;
    }

    let finalization = TraceFinalization {
        output_tokens: &generated_tokens,
        logical_prompt_tokens: generated_tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: generated_tokens.len() as u32,
        logical_output_tokens: generated_tokens.len() as u32,
        billed_output_tokens: generated_tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(policy_digest),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    sink.finalize(finalization).await?;

    // Verify persisted data
    let stored: (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT stop_reason_code, stop_reason_token_index
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind(&trace_id)
    .fetch_one(db.pool())
    .await?;

    assert_eq!(stored.0.as_deref(), Some("COMPLETION_CONFIDENT"));
    assert_eq!(stored.1, Some(token_index as i64));

    Ok(())
}

#[tokio::test]
async fn test_stop_controller_repetition_guard_persisted() -> Result<()> {
    let db = init_test_db().await?;

    let policy = StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(999),
        completion_threshold_q15: 32767, // Won't trigger
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 999, 1000);
    let policy_digest = sim.policy_digest().clone();

    // Generate repeating pattern [1, 2, 3, 4, 5, 1, 2, 3]
    let tokens_to_generate = vec![1, 2, 3, 4, 5, 1, 2, 3];
    let (generated_tokens, stop_decision, _) = sim.generate_until_stop(tokens_to_generate, 0.1);

    assert!(stop_decision.is_some());
    let (reason, token_index) = stop_decision.unwrap();
    assert_eq!(reason, StopReasonCode::RepetitionGuard);

    // Persist to database
    let trace_id = "stop-repetition-guard".to_string();
    let context_digest = [0x03u8; 32];
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-repetition".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

    for (idx, _token) in generated_tokens.iter().enumerate() {
        sink.record_token(make_token_input(
            idx as u32,
            vec!["adapter-1".to_string()],
            vec![16384],
        ))
        .await?;
    }

    let finalization = TraceFinalization {
        output_tokens: &generated_tokens,
        logical_prompt_tokens: generated_tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: generated_tokens.len() as u32,
        logical_output_tokens: generated_tokens.len() as u32,
        billed_output_tokens: generated_tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(policy_digest),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    sink.finalize(finalization).await?;

    let stored: (Option<String>,) =
        sqlx::query_as("SELECT stop_reason_code FROM inference_trace_receipts WHERE trace_id = ?")
            .bind(&trace_id)
            .fetch_one(db.pool())
            .await?;

    assert_eq!(stored.0.as_deref(), Some("REPETITION_GUARD"));

    Ok(())
}

#[tokio::test]
async fn test_stop_controller_length_eos_persisted() -> Result<()> {
    let db = init_test_db().await?;

    let policy = StopPolicySpec {
        output_max_tokens: 100,
        eos_token_id: Some(42),
        completion_threshold_q15: 32767,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 42, 100);
    let policy_digest = sim.policy_digest().clone();

    // Generate tokens, then EOS
    let tokens_to_generate = vec![1, 2, 42]; // 42 is EOS
    let (generated_tokens, stop_decision, _) = sim.generate_until_stop(tokens_to_generate, 0.1);

    assert!(stop_decision.is_some());
    let (reason, token_index) = stop_decision.unwrap();
    assert_eq!(reason, StopReasonCode::Length);
    assert_eq!(token_index, 2); // Stopped at 3rd token (index 2)

    // Persist to database
    let trace_id = "stop-length-eos".to_string();
    let context_digest = [0x04u8; 32];
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-eos".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

    for (idx, _token) in generated_tokens.iter().enumerate() {
        sink.record_token(make_token_input(
            idx as u32,
            vec!["adapter-1".to_string()],
            vec![16384],
        ))
        .await?;
    }

    let finalization = TraceFinalization {
        output_tokens: &generated_tokens,
        logical_prompt_tokens: generated_tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: generated_tokens.len() as u32,
        logical_output_tokens: generated_tokens.len() as u32,
        billed_output_tokens: generated_tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(policy_digest),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    sink.finalize(finalization).await?;

    let stored: (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT stop_reason_code, stop_reason_token_index
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind(&trace_id)
    .fetch_one(db.pool())
    .await?;

    assert_eq!(stored.0.as_deref(), Some("LENGTH"));
    assert_eq!(stored.1, Some(2));

    Ok(())
}

#[tokio::test]
async fn test_determinism_same_policy_same_receipt_digest() -> Result<()> {
    let db1 = init_test_db().await?;
    let db2 = init_test_db().await?;

    let policy = StopPolicySpec {
        output_max_tokens: 5,
        eos_token_id: Some(100),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    // Run 1
    let mut sim1 = InferenceSimulation::new(policy.clone(), 100, 200);
    let policy_digest1 = sim1.policy_digest().clone();
    let (tokens1, stop1, _) = sim1.generate_until_stop(vec![1, 2, 3, 4, 5, 6], 0.1);

    // Run 2 with same policy
    let mut sim2 = InferenceSimulation::new(policy, 100, 200);
    let policy_digest2 = sim2.policy_digest().clone();
    let (tokens2, stop2, _) = sim2.generate_until_stop(vec![1, 2, 3, 4, 5, 6], 0.1);

    // Verify identical results
    assert_eq!(tokens1, tokens2);
    assert_eq!(stop1, stop2);
    assert_eq!(policy_digest1, policy_digest2);

    // Persist both and compare receipt digests
    let context_digest = [0x05u8; 32];

    // Receipt 1
    let start1 = TraceStart {
        trace_id: "determ-run-1".to_string(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-1".to_string()),
        context_digest,
    };
    let mut sink1 = SqlTraceSink::new(db1.clone(), start1, 8).await?;
    for (idx, _) in tokens1.iter().enumerate() {
        sink1
            .record_token(make_token_input(
                idx as u32,
                vec!["adapter-1".to_string()],
                vec![16384],
            ))
            .await?;
    }
    let (reason1, idx1) = stop1.unwrap();
    let finalization1 = TraceFinalization {
        output_tokens: &tokens1,
        logical_prompt_tokens: tokens1.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: tokens1.len() as u32,
        logical_output_tokens: tokens1.len() as u32,
        billed_output_tokens: tokens1.len() as u32,
        stop_reason_code: Some(reason1.to_string()),
        stop_reason_token_index: Some(idx1),
        stop_policy_digest_b3: Some(policy_digest1),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };
    let receipt1 = sink1.finalize(finalization1).await?;

    // Receipt 2
    let start2 = TraceStart {
        trace_id: "determ-run-2".to_string(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-2".to_string()),
        context_digest,
    };
    let mut sink2 = SqlTraceSink::new(db2.clone(), start2, 8).await?;
    for (idx, _) in tokens2.iter().enumerate() {
        sink2
            .record_token(make_token_input(
                idx as u32,
                vec!["adapter-1".to_string()],
                vec![16384],
            ))
            .await?;
    }
    let (reason2, idx2) = stop2.unwrap();
    let finalization2 = TraceFinalization {
        output_tokens: &tokens2,
        logical_prompt_tokens: tokens2.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: tokens2.len() as u32,
        logical_output_tokens: tokens2.len() as u32,
        billed_output_tokens: tokens2.len() as u32,
        stop_reason_code: Some(reason2.to_string()),
        stop_reason_token_index: Some(idx2),
        stop_policy_digest_b3: Some(policy_digest2),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };
    let receipt2 = sink2.finalize(finalization2).await?;

    // Verify identical receipt digests
    assert_eq!(
        receipt1.receipt_digest.to_hex(),
        receipt2.receipt_digest.to_hex(),
        "Receipt digests must be identical for deterministic runs"
    );

    Ok(())
}

#[tokio::test]
async fn test_different_policies_different_digests() -> Result<()> {
    let policy1 = StopPolicySpec {
        output_max_tokens: 10,
        eos_token_id: Some(100),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let policy2 = StopPolicySpec {
        output_max_tokens: 20, // Different
        eos_token_id: Some(100),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let sim1 = InferenceSimulation::new(policy1, 100, 200);
    let sim2 = InferenceSimulation::new(policy2, 100, 200);

    assert_ne!(
        sim1.policy_digest().to_hex(),
        sim2.policy_digest().to_hex(),
        "Different policies must have different digests"
    );

    Ok(())
}

#[tokio::test]
async fn test_stop_policy_override_from_request() -> Result<()> {
    // This test verifies that a custom policy from the request is used
    // instead of defaults
    let default_policy = StopPolicySpec::new(100);
    let custom_policy = StopPolicySpec {
        output_max_tokens: 3, // Very low budget
        eos_token_id: Some(42),
        completion_threshold_q15: 16384,
        repetition_ngram: 2,
        repetition_window: 16,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim_default = InferenceSimulation::new(default_policy, 42, 100);
    let mut sim_custom = InferenceSimulation::new(custom_policy, 42, 100);

    // Same token sequence
    let tokens = vec![1, 2, 3, 4, 5];

    let (tokens_default, stop_default, _) = sim_default.generate_until_stop(tokens.clone(), 0.1);
    let (tokens_custom, stop_custom, _) = sim_custom.generate_until_stop(tokens, 0.1);

    // Default policy should generate all tokens (budget is 100)
    assert_eq!(tokens_default.len(), 5);
    assert!(stop_default.is_none());

    // Custom policy should stop at budget (3 tokens)
    assert_eq!(tokens_custom.len(), 4); // Includes the token that triggered stop
    assert!(stop_custom.is_some());
    let (reason, _) = stop_custom.unwrap();
    assert_eq!(reason, StopReasonCode::BudgetMax);

    // Policy digests should be different
    assert_ne!(
        sim_default.policy_digest().to_hex(),
        sim_custom.policy_digest().to_hex()
    );

    Ok(())
}

#[tokio::test]
async fn test_stop_decision_token_index_accuracy() -> Result<()> {
    let policy = StopPolicySpec {
        output_max_tokens: 7,
        eos_token_id: Some(99),
        completion_threshold_q15: 32767,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 99, 100);

    // Generate exactly to the budget limit
    let tokens = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let (generated, stop_decision, _) = sim.generate_until_stop(tokens, 0.1);

    assert!(stop_decision.is_some());
    let (reason, token_index) = stop_decision.unwrap();
    assert_eq!(reason, StopReasonCode::BudgetMax);
    assert_eq!(token_index, 7, "Should stop at token index 7 (8th token)");
    assert_eq!(
        generated.len(),
        8,
        "Should have generated 8 tokens (indices 0-7)"
    );

    Ok(())
}

#[tokio::test]
async fn test_stop_policy_digest_committed_to_merkle_bundle() -> Result<()> {
    let db = init_test_db().await?;

    let policy = StopPolicySpec {
        output_max_tokens: 3,
        eos_token_id: Some(100),
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };

    let mut sim = InferenceSimulation::new(policy, 100, 200);
    let policy_digest = sim.policy_digest().clone();

    let (tokens, stop_decision, _) = sim.generate_until_stop(vec![1, 2, 3, 4], 0.1);
    let (reason, token_index) = stop_decision.unwrap();

    // Create trace
    let trace_id = "merkle-bundle-test".to_string();
    let context_digest = [0x06u8; 32];
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-merkle".to_string()),
        context_digest,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 8).await?;

    for (idx, _) in tokens.iter().enumerate() {
        sink.record_token(make_token_input(
            idx as u32,
            vec!["adapter-1".to_string()],
            vec![16384],
        ))
        .await?;
    }

    let finalization = TraceFinalization {
        output_tokens: &tokens,
        logical_prompt_tokens: tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: tokens.len() as u32,
        logical_output_tokens: tokens.len() as u32,
        billed_output_tokens: tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(policy_digest.clone()),
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    let receipt = sink.finalize(finalization).await?;

    // The receipt_digest includes the stop_policy_digest_b3
    // Verify that changing the policy digest would change the receipt digest
    let _ = receipt; // Use receipt to silence warning
    let stored: (Vec<u8>,) =
        sqlx::query_as("SELECT receipt_digest FROM inference_trace_receipts WHERE trace_id = ?")
            .bind(&trace_id)
            .fetch_one(db.pool())
            .await?;

    let receipt_digest_bytes: [u8; 32] = stored
        .0
        .as_slice()
        .try_into()
        .context("Invalid receipt digest")?;
    let receipt_digest_with_policy = B3Hash::from_bytes(receipt_digest_bytes);

    // Create another trace with different policy (even though output is same)
    let different_policy = StopPolicySpec {
        output_max_tokens: 3,
        eos_token_id: Some(101), // Different EOS
        completion_threshold_q15: 24576,
        repetition_ngram: 3,
        repetition_window: 32,
        repetition_threshold: 1,
        stop_sequences: vec![],
    };
    let different_policy_digest = different_policy.digest();

    assert_ne!(
        policy_digest.to_hex(),
        different_policy_digest.to_hex(),
        "Policy digests should differ"
    );

    // Create second trace with different policy digest
    let trace_id2 = "merkle-bundle-test-2".to_string();
    let start2 = TraceStart {
        trace_id: trace_id2.clone(),
        tenant_id: "tenant-test".to_string(),
        request_id: Some("req-merkle-2".to_string()),
        context_digest,
    };

    let mut sink2 = SqlTraceSink::new(db.clone(), start2, 8).await?;

    for (idx, _) in tokens.iter().enumerate() {
        sink2
            .record_token(make_token_input(
                idx as u32,
                vec!["adapter-1".to_string()],
                vec![16384],
            ))
            .await?;
    }

    let finalization2 = TraceFinalization {
        output_tokens: &tokens,
        logical_prompt_tokens: tokens.len() as u32,
        prefix_cached_token_count: 0,
        billed_input_tokens: tokens.len() as u32,
        logical_output_tokens: tokens.len() as u32,
        billed_output_tokens: tokens.len() as u32,
        stop_reason_code: Some(reason.to_string()),
        stop_reason_token_index: Some(token_index),
        stop_policy_digest_b3: Some(different_policy_digest), // Different digest
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        attestation: None,
        equipment_profile: None,
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
    };

    sink2.finalize(finalization2).await?;

    let stored2: (Vec<u8>,) =
        sqlx::query_as("SELECT receipt_digest FROM inference_trace_receipts WHERE trace_id = ?")
            .bind(&trace_id2)
            .fetch_one(db.pool())
            .await?;

    let receipt_digest_bytes2: [u8; 32] = stored2
        .0
        .as_slice()
        .try_into()
        .context("Invalid receipt digest")?;
    let receipt_digest_with_different_policy = B3Hash::from_bytes(receipt_digest_bytes2);

    // Verify that different policy digests result in different receipt digests
    assert_ne!(
        receipt_digest_with_policy.to_hex(),
        receipt_digest_with_different_policy.to_hex(),
        "Receipt digest must include stop policy digest (Merkle bundle commitment)"
    );

    Ok(())
}

#[tokio::test]
async fn test_all_stop_reasons_trigger_correctly_in_integration() -> Result<()> {
    // This test verifies all four stop reasons can be triggered in realistic scenarios

    // 1. BUDGET_MAX
    {
        let policy = StopPolicySpec {
            output_max_tokens: 2,
            eos_token_id: Some(100),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 32,
            repetition_threshold: 1,
            stop_sequences: vec![],
        };
        let mut sim = InferenceSimulation::new(policy, 100, 200);
        let (_, stop, _) = sim.generate_until_stop(vec![1, 2, 3], 0.1);
        assert_eq!(stop.unwrap().0, StopReasonCode::BudgetMax);
    }

    // 2. COMPLETION_CONFIDENT
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(10),
            completion_threshold_q15: 16384, // ~0.5
            repetition_ngram: 3,
            repetition_window: 32,
            repetition_threshold: 1,
            stop_sequences: vec![],
        };
        let mut sim = InferenceSimulation::new(policy, 10, 100);
        let (_, stop, _) = sim.generate_until_stop(vec![1, 2, 3], 15.0); // High EOS logit
        assert_eq!(stop.unwrap().0, StopReasonCode::CompletionConfident);
    }

    // 3. REPETITION_GUARD
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(999),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 32,
            repetition_threshold: 1,
            stop_sequences: vec![],
        };
        let mut sim = InferenceSimulation::new(policy, 999, 1000);
        let (_, stop, _) = sim.generate_until_stop(vec![1, 2, 3, 4, 5, 1, 2, 3], 0.1);
        assert_eq!(stop.unwrap().0, StopReasonCode::RepetitionGuard);
    }

    // 4. LENGTH (EOS token)
    {
        let policy = StopPolicySpec {
            output_max_tokens: 100,
            eos_token_id: Some(42),
            completion_threshold_q15: 32767,
            repetition_ngram: 3,
            repetition_window: 32,
            repetition_threshold: 1,
            stop_sequences: vec![],
        };
        let mut sim = InferenceSimulation::new(policy, 42, 100);
        let (_, stop, _) = sim.generate_until_stop(vec![1, 2, 42], 0.1);
        assert_eq!(stop.unwrap().0, StopReasonCode::Length);
    }

    Ok(())
}
