use adapteros_core::B3Hash;
use adapteros_db::{recompute_receipt, SqlTraceSink, TraceStart, TraceTokenInput};
use std::sync::Arc;

fn encode_gates(values: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + values.len() * 2);
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[tokio::test]
async fn trace_persistence_and_receipt_verification() -> anyhow::Result<()> {
    let db = Arc::new(adapteros_db::Db::new_in_memory().await?);
    let context_digest = B3Hash::hash(b"context-1").to_bytes();
    let trace_id = "trace-1".to_string();

    let start = TraceStart {
        trace_id: trace_id.clone(),
        tenant_id: "tenant-1".to_string(),
        request_id: Some("req-1".to_string()),
        context_digest,
    };
    let mut sink = SqlTraceSink::new(db.clone(), start, 32).await?;

    let mut token_inputs: Vec<TraceTokenInput> = vec![
        TraceTokenInput {
            token_index: 0,
            adapter_ids: vec!["adapter-a".into()],
            gates_q15: vec![123],
            policy_mask_digest: Some(B3Hash::hash(b"mask").to_bytes()),
            backend_id: Some("coreml".into()),
            kernel_version_id: Some("v1".into()),
        },
        TraceTokenInput {
            token_index: 1,
            adapter_ids: vec!["adapter-b".into(), "adapter-c".into()],
            gates_q15: vec![321, 111],
            policy_mask_digest: Some(B3Hash::hash(b"mask").to_bytes()),
            backend_id: Some("coreml".into()),
            kernel_version_id: Some("v1".into()),
        },
        TraceTokenInput {
            token_index: 2,
            adapter_ids: vec!["adapter-a".into()],
            gates_q15: vec![99],
            policy_mask_digest: Some(B3Hash::hash(b"mask").to_bytes()),
            backend_id: Some("coreml".into()),
            kernel_version_id: Some("v1".into()),
        },
    ];

    for input in &token_inputs {
        sink.record_token(input.clone()).await?;
    }

    let receipt = sink.finalize(&[11, 22, 33]).await?;

    let token_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM inference_trace_tokens WHERE trace_id = ?")
            .bind(&trace_id)
            .fetch_one(db.pool())
            .await?;
    assert_eq!(token_count, 3);

    let verification = recompute_receipt(&db, &trace_id).await?;
    assert!(verification.matches);
    assert!(verification.mismatched_token.is_none());
    assert_eq!(
        verification
            .stored
            .as_ref()
            .map(|r| r.receipt_digest.to_hex()),
        Some(receipt.receipt_digest.to_hex())
    );

    // Tamper with a token row to force mismatch detection
    let tampered = encode_gates(&[777]);
    sqlx::query(
        "UPDATE inference_trace_tokens SET gates_q15 = ? WHERE trace_id = ? AND token_index = 1",
    )
    .bind(tampered)
    .bind(&trace_id)
    .execute(db.pool())
    .await?;

    let tampered_verification = recompute_receipt(&db, &trace_id).await?;
    assert!(!tampered_verification.matches);
    assert_eq!(tampered_verification.mismatched_token, Some(1));

    // Deterministic rerun yields identical receipt digest
    let start_second = TraceStart {
        trace_id: "trace-2".to_string(),
        tenant_id: "tenant-1".to_string(),
        request_id: Some("req-1".to_string()),
        context_digest,
    };
    let mut sink_second = SqlTraceSink::new(db.clone(), start_second, 32).await?;
    for input in token_inputs.drain(..) {
        sink_second.record_token(input).await?;
    }
    let receipt_second = sink_second.finalize(&[11, 22, 33]).await?;
    assert_eq!(
        receipt.receipt_digest.to_hex(),
        receipt_second.receipt_digest.to_hex()
    );

    Ok(())
}
