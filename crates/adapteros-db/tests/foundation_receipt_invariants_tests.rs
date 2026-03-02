use adapteros_core::{B3Hash, EquipmentProfile};
use adapteros_db::{
    recompute_receipt, sqlx, Db, SqlTraceSink, TraceFinalization, TraceSink, TraceStart,
    TraceTokenInput,
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn adapter_swap_receipt_is_audited_with_required_fields() -> anyhow::Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = "default";

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind("Default")
        .execute(db.pool_result()?)
        .await?;

    let metadata = json!({
        "old_adapter_id": "adp-old",
        "new_adapter_id": "adp-new",
        "duration_ms": 13,
        "vram_delta_mb": 1,
    })
    .to_string();

    db.log_audit(
        "dev-no-auth",
        "admin",
        tenant_id,
        "adapter.swap",
        "adapter",
        Some("adp-old -> adp-new"),
        "success",
        None,
        Some("127.0.0.1"),
        Some(&metadata),
    )
    .await?;

    let logs = db
        .query_audit_logs_for_tenant(
            tenant_id,
            Some("dev-no-auth"),
            Some("adapter.swap"),
            Some("adapter"),
            None,
            None,
            10,
        )
        .await?;

    assert_eq!(logs.len(), 1, "expected one swap receipt in audit logs");

    let log = &logs[0];
    assert_eq!(log.action, "adapter.swap");
    assert_eq!(log.resource_id.as_deref(), Some("adp-old -> adp-new"));
    assert_eq!(log.status, "success");

    let parsed: serde_json::Value =
        serde_json::from_str(log.metadata_json.as_deref().unwrap_or("{}"))?;
    assert_eq!(
        parsed.get("old_adapter_id").and_then(|v| v.as_str()),
        Some("adp-old")
    );
    assert_eq!(
        parsed.get("new_adapter_id").and_then(|v| v.as_str()),
        Some("adp-new")
    );

    assert!(db.verify_audit_chain_for_tenant(tenant_id).await?);

    Ok(())
}

#[tokio::test]
async fn inference_finalize_persists_run_receipt_fields() -> anyhow::Result<()> {
    let db = Arc::new(Db::new_in_memory().await?);
    let tenant_id = "tenant-1";

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind("Tenant One")
        .execute(db.pool_result()?)
        .await?;

    let trace_id = "trace-foundation-receipt".to_string();
    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: tenant_id.to_string(),
        request_id: Some("req-foundation-receipt".to_string()),
        context_digest: B3Hash::hash(b"context-foundation").to_bytes(),
        stack_id: None,
        model_id: None,
        policy_id: None,
    };

    let mut sink = SqlTraceSink::new(db.clone(), start, 4).await?;

    sink.record_token(TraceTokenInput {
        token_index: 0,
        adapter_ids: vec!["adapter-foundation".to_string()],
        gates_q15: vec![32767],
        policy_mask_digest_b3: None,
        allowed_mask: None,
        policy_overrides_applied: None,
        backend_id: Some("mlx".to_string()),
        kernel_version_id: Some("v1".to_string()),
    })
    .await?;

    let output_tokens = [42u32];
    let equipment_profile =
        EquipmentProfile::compute("Apple M4 Max", "0.14.1", Some("ANEv4-38core"));
    let receipt = sink
        .finalize(TraceFinalization {
            output_tokens: &output_tokens,
            logical_prompt_tokens: 3,
            prefix_cached_token_count: 0,
            billed_input_tokens: 3,
            logical_output_tokens: 1,
            billed_output_tokens: 1,
            stop_reason_code: Some("stop".to_string()),
            stop_reason_token_index: Some(0),
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
            equipment_profile: Some(equipment_profile.clone()),
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

    let (receipt_digest, run_head_hash, logical_output_tokens, billed_output_tokens):
        (Vec<u8>, Vec<u8>, i64, i64) = sqlx::query_as(
            "SELECT receipt_digest, run_head_hash, logical_output_tokens, billed_output_tokens FROM inference_trace_receipts WHERE trace_id = ?",
        )
        .bind(&trace_id)
        .fetch_one(db.pool_result()?)
        .await?;

    assert_eq!(
        receipt_digest.len(),
        32,
        "receipt digest must be BLAKE3 length"
    );
    assert_eq!(
        run_head_hash.len(),
        32,
        "run head hash must be BLAKE3 length"
    );
    assert_eq!(logical_output_tokens, 1);
    assert_eq!(billed_output_tokens, 1);

    let verification = recompute_receipt(&db, &trace_id).await?;
    assert!(verification.matches, "stored receipt should verify");
    let stored_profile = verification
        .stored
        .as_ref()
        .and_then(|stored| stored.equipment_profile.as_ref())
        .expect("equipment profile should round-trip through receipt recomputation");
    assert_eq!(stored_profile.digest, equipment_profile.digest);
    assert_eq!(stored_profile.processor_id, equipment_profile.processor_id);
    assert_eq!(
        stored_profile.engine_version,
        equipment_profile.engine_version
    );
    assert_eq!(stored_profile.ane_version, equipment_profile.ane_version);
    assert_eq!(
        verification
            .stored
            .as_ref()
            .map(|stored| stored.receipt_digest.to_hex()),
        Some(receipt.receipt_digest.to_hex())
    );

    Ok(())
}
