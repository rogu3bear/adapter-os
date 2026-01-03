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
        placement_trace: None,
        stop_reason_code: None,
        stop_reason_token_index: None,
        stop_policy_digest_b3: None,
    };

    let resolved = InferenceCore::resolve_token_usage(&worker_response).expect("token usage");
    assert_eq!(resolved.prompt_tokens, logical_prompt_tokens);
    assert_eq!(resolved.completion_tokens, logical_output_tokens);
    assert_eq!(resolved.billed_input_tokens, billed_input_tokens);
    assert_eq!(resolved.billed_output_tokens, billed_output_tokens);
}
