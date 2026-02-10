use crate::inference_core::InferenceCore;
use crate::types::{RouterSummary, TokenUsage, WorkerInferResponse, WorkerTrace};
use adapteros_api_types::RunReceipt;
use adapteros_core::B3Hash;

#[test]
fn token_usage_prefers_run_receipt_over_worker_usage() {
    let logical_prompt_tokens = 7;
    let logical_output_tokens = 3;
    let billed_input_tokens = 5;
    let billed_output_tokens = 3;
    let run_receipt = RunReceipt {
        trace_id: "trace-usage".to_string(),
        run_head_hash: B3Hash::hash(b"run-head"),
        output_digest: B3Hash::hash(b"output"),
        receipt_digest: B3Hash::hash(b"receipt"),
        signature: None,
        attestation: None,
        logical_prompt_tokens,
        prefix_cached_token_count: 2,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        previous_receipt_digest: None,
        session_sequence: 0,
        tokenizer_hash_b3: None,
        tokenizer_version: None,
        tokenizer_normalization: None,
        model_build_hash_b3: None,
        adapter_build_hash_b3: None,
        decode_algo: None,
        temperature_q15: None,
        top_p_q15: None,
        top_k: None,
        seed_digest_b3: None,
        sampling_backend: None,
        thread_count: None,
        reduction_strategy: None,
        stop_eos_q15: None,
        stop_window_digest_b3: None,
        cache_scope: None,
        cached_prefix_digest_b3: None,
        cached_prefix_len: None,
        cache_key_b3: None,
        retrieval_merkle_root_b3: None,
        retrieval_order_digest_b3: None,
        tool_call_inputs_digest_b3: None,
        tool_call_outputs_digest_b3: None,
        disclosure_level: None,
        receipt_signing_kid: None,
        receipt_signed_at: None,
    };

    let backend_usage = TokenUsage {
        prompt_tokens: 999,
        completion_tokens: 888,
        billed_input_tokens: 777,
        billed_output_tokens: 666,
    };

    let worker_response = WorkerInferResponse {
        text: Some("ok".to_string()),
        status: "stop".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![],
            },
            token_count: 0,
            router_decisions: None,
            router_decision_chain: None,
            model_type: None,
        },
        run_receipt: Some(run_receipt),
        token_usage: Some(backend_usage),
        backend_used: None,
        backend_version: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        pinned_degradation_evidence: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tokenizer_digest_b3: None,
        backend_raw: None,
    };

    let resolved = InferenceCore::resolve_token_usage(&worker_response).expect("token usage");
    assert_eq!(resolved.prompt_tokens, logical_prompt_tokens);
    assert_eq!(resolved.completion_tokens, logical_output_tokens);
    assert_eq!(resolved.billed_input_tokens, billed_input_tokens);
    assert_eq!(resolved.billed_output_tokens, billed_output_tokens);
}

/// Helper: build a WorkerInferResponse with the given receipt and token_usage.
fn make_worker_response(
    run_receipt: Option<RunReceipt>,
    token_usage: Option<TokenUsage>,
) -> WorkerInferResponse {
    WorkerInferResponse {
        text: Some("ok".to_string()),
        status: "stop".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![],
            },
            token_count: 0,
            router_decisions: None,
            router_decision_chain: None,
            model_type: None,
        },
        run_receipt,
        token_usage,
        backend_used: None,
        backend_version: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        fallback_backend: None,
        determinism_mode_applied: None,
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        pinned_degradation_evidence: None,
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tokenizer_digest_b3: None,
        backend_raw: None,
    }
}

/// Helper: build a RunReceipt with the given accounting fields; non-accounting fields use defaults.
fn make_receipt(
    logical_prompt_tokens: u32,
    prefix_cached_token_count: u32,
    billed_input_tokens: u32,
    logical_output_tokens: u32,
    billed_output_tokens: u32,
) -> RunReceipt {
    RunReceipt {
        trace_id: "trace-usage-test".to_string(),
        run_head_hash: B3Hash::hash(b"run-head"),
        output_digest: B3Hash::hash(b"output"),
        receipt_digest: B3Hash::hash(b"receipt"),
        signature: None,
        attestation: None,
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
        tenant_kv_quota_bytes: 0,
        tenant_kv_bytes_used: 0,
        kv_evictions: 0,
        kv_residency_policy_id: None,
        kv_quota_enforced: false,
        prefix_kv_key_b3: None,
        prefix_cache_hit: false,
        prefix_kv_bytes: 0,
        model_cache_identity_v2_digest_b3: None,
        previous_receipt_digest: None,
        session_sequence: 0,
        tokenizer_hash_b3: None,
        tokenizer_version: None,
        tokenizer_normalization: None,
        model_build_hash_b3: None,
        adapter_build_hash_b3: None,
        decode_algo: None,
        temperature_q15: None,
        top_p_q15: None,
        top_k: None,
        seed_digest_b3: None,
        sampling_backend: None,
        thread_count: None,
        reduction_strategy: None,
        stop_eos_q15: None,
        stop_window_digest_b3: None,
        cache_scope: None,
        cached_prefix_digest_b3: None,
        cached_prefix_len: None,
        cache_key_b3: None,
        retrieval_merkle_root_b3: None,
        retrieval_order_digest_b3: None,
        tool_call_inputs_digest_b3: None,
        tool_call_outputs_digest_b3: None,
        disclosure_level: None,
        receipt_signing_kid: None,
        receipt_signed_at: None,
    }
}

#[test]
fn test_resolve_token_usage_no_receipt_falls_back_to_worker() {
    let worker_usage = TokenUsage {
        prompt_tokens: 200,
        completion_tokens: 100,
        billed_input_tokens: 200,
        billed_output_tokens: 100,
    };
    let resp = make_worker_response(None, Some(worker_usage));

    let resolved = InferenceCore::resolve_token_usage(&resp).expect("should fall back to worker");
    assert_eq!(resolved.prompt_tokens, 200);
    assert_eq!(resolved.completion_tokens, 100);
    assert_eq!(resolved.billed_input_tokens, 200);
    assert_eq!(resolved.billed_output_tokens, 100);
}

#[test]
fn test_resolve_token_usage_both_none_returns_none() {
    let resp = make_worker_response(None, None);
    assert!(InferenceCore::resolve_token_usage(&resp).is_none());
}

#[test]
fn test_resolve_token_usage_receipt_with_zero_cached() {
    let receipt = make_receipt(500, 0, 500, 200, 200);
    let worker_usage = TokenUsage {
        prompt_tokens: 999,
        completion_tokens: 888,
        billed_input_tokens: 777,
        billed_output_tokens: 666,
    };
    let resp = make_worker_response(Some(receipt), Some(worker_usage));

    let resolved = InferenceCore::resolve_token_usage(&resp).expect("receipt should win");
    assert_eq!(resolved.prompt_tokens, 500);
    assert_eq!(resolved.completion_tokens, 200);
    assert_eq!(resolved.billed_input_tokens, 500);
    assert_eq!(resolved.billed_output_tokens, 200);
}

#[test]
fn test_resolve_token_usage_receipt_with_full_cache() {
    let receipt = make_receipt(500, 500, 0, 200, 200);
    let worker_usage = TokenUsage {
        prompt_tokens: 999,
        completion_tokens: 888,
        billed_input_tokens: 777,
        billed_output_tokens: 666,
    };
    let resp = make_worker_response(Some(receipt), Some(worker_usage));

    let resolved = InferenceCore::resolve_token_usage(&resp).expect("receipt should win");
    assert_eq!(resolved.prompt_tokens, 500);
    assert_eq!(resolved.completion_tokens, 200);
    assert_eq!(resolved.billed_input_tokens, 0);
    assert_eq!(resolved.billed_output_tokens, 200);
}

#[test]
fn test_resolve_token_usage_receipt_with_partial_cache() {
    let receipt = make_receipt(1000, 400, 600, 300, 300);
    let worker_usage = TokenUsage {
        prompt_tokens: 999,
        completion_tokens: 888,
        billed_input_tokens: 777,
        billed_output_tokens: 666,
    };
    let resp = make_worker_response(Some(receipt), Some(worker_usage));

    let resolved = InferenceCore::resolve_token_usage(&resp).expect("receipt should win");
    assert_eq!(resolved.prompt_tokens, 1000);
    assert_eq!(resolved.completion_tokens, 300);
    assert_eq!(resolved.billed_input_tokens, 600);
    assert_eq!(resolved.billed_output_tokens, 300);
}
