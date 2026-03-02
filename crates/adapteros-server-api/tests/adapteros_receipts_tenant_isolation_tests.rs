use adapteros_db::sqlx;
use adapteros_db::{SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers::adapteros_receipts::{
    adapteros_replay, get_receipt_by_digest, AdapterosReplayRequest,
};
use adapteros_server_api::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};

mod common;
use common::{setup_state, test_admin_claims, TestkitEnvGuard};

async fn seed_receipt_digest_hex(
    state: &AppState,
    tenant_id: &str,
    trace_id: &str,
) -> anyhow::Result<String> {
    let start = TraceStart {
        trace_id: trace_id.to_string(),
        tenant_id: tenant_id.to_string(),
        request_id: Some(trace_id.to_string()),
        context_digest: [7u8; 32],
        stack_id: None,
        model_id: None,
        policy_id: None,
    };

    let mut sink = SqlTraceSink::new(state.db.as_db_arc(), start, 1).await?;
    sink.record_token(TraceTokenInput {
        token_index: 0,
        adapter_ids: vec!["adapter-a".to_string()],
        gates_q15: vec![123],
        policy_mask_digest_b3: None,
        allowed_mask: None,
        policy_overrides_applied: None,
        backend_id: None,
        kernel_version_id: None,
    })
    .await?;

    let output_tokens = [1u32, 2u32];
    sink.finalize(TraceFinalization {
        output_tokens: &output_tokens,
        logical_prompt_tokens: 1,
        prefix_cached_token_count: 0,
        billed_input_tokens: 1,
        logical_output_tokens: output_tokens.len() as u32,
        billed_output_tokens: output_tokens.len() as u32,
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
        attestation: None,
        equipment_profile: None,
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
        cache_attestation: None,
        worker_public_key: None,
        copy_bytes: None,
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
    })
    .await?;
    sink.flush().await?;

    let digest_hex: String = sqlx::query_scalar(
        "SELECT lower(hex(receipt_digest)) FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind(trace_id)
    .fetch_one(state.db.pool_result()?)
    .await?;
    Ok(digest_hex)
}

fn cross_tenant_claims() -> Claims {
    let mut claims = test_admin_claims();
    claims.tenant_id = "default".to_string();
    claims.admin_tenants = vec![];
    claims
}

#[tokio::test]
async fn get_receipt_by_digest_returns_forbidden_cross_tenant() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let digest_hex = seed_receipt_digest_hex(&state, "tenant-1", "trace-receipt-cross-get").await?;

    let result = get_receipt_by_digest(
        State(state.clone()),
        Extension(cross_tenant_claims()),
        Path(digest_hex),
    )
    .await;

    match result {
        Err(err) => assert_eq!(err.status, StatusCode::FORBIDDEN),
        Ok(_) => panic!("Cross-tenant receipt retrieval must be forbidden"),
    }

    Ok(())
}

#[tokio::test]
async fn adapteros_replay_returns_forbidden_cross_tenant() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let digest_hex =
        seed_receipt_digest_hex(&state, "tenant-1", "trace-receipt-cross-replay").await?;

    let result = adapteros_replay(
        State(state.clone()),
        Extension(cross_tenant_claims()),
        Json(AdapterosReplayRequest {
            receipt_digest: Some(digest_hex),
            payload: None,
        }),
    )
    .await;

    match result {
        Err(err) => assert_eq!(err.status, StatusCode::FORBIDDEN),
        Ok(_) => panic!("Cross-tenant replay verification must be forbidden"),
    }

    Ok(())
}

#[tokio::test]
async fn get_receipt_by_digest_returns_same_tenant_receipt() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let trace_id = "trace-receipt-same-tenant";
    let digest_hex = seed_receipt_digest_hex(&state, "tenant-1", trace_id).await?;

    let response = get_receipt_by_digest(
        State(state.clone()),
        Extension(test_admin_claims()),
        Path(digest_hex),
    )
    .await;
    let Json(receipt) = match response {
        Ok(json) => json,
        Err(err) => panic!(
            "Same-tenant receipt retrieval should succeed: {}",
            err.message
        ),
    };

    assert_eq!(receipt.trace_id, trace_id);
    Ok(())
}
