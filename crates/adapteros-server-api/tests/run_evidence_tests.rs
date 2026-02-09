use adapteros_core::B3Hash;
use adapteros_db::replay_metadata::CreateReplayMetadataParams;
use adapteros_db::{SqlTraceSink, TraceFinalization, TraceSink, TraceStart, TraceTokenInput};
use adapteros_server_api::handlers::run_evidence::{download_run_evidence, EvidenceExportParams};
use adapteros_server_api::types::SamplingParams;
use axum::body::to_bytes;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension,
};
use std::io::Read;
use zip::ZipArchive;

mod common;
use common::{setup_state, test_admin_claims, TestkitEnvGuard};

#[tokio::test]
async fn evidence_bundle_contains_required_files() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();
    let tenant_id = claims.tenant_id.clone();
    let run_id = "run-evidence-001".to_string();

    // Seed manifest referenced by replay metadata
    let manifest_body = r#"{"name":"test-manifest","version":"1"}"#;
    let manifest_hash = B3Hash::hash(manifest_body.as_bytes()).to_hex();
    state
        .db
        .create_manifest(&tenant_id, &manifest_hash, manifest_body)
        .await?;

    // Create replay metadata for the run
    let sampling_params_json = serde_json::to_string(&SamplingParams::default()).unwrap();
    let replay_params = CreateReplayMetadataParams {
        inference_id: run_id.clone(),
        tenant_id: tenant_id.clone(),
        manifest_hash: manifest_hash.clone(),
        base_model_id: Some("base-model".to_string()),
        router_seed: Some("seed-123".to_string()),
        sampling_params_json,
        backend: "metal".to_string(),
        backend_version: Some("v1.0.0".to_string()),
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        sampling_algorithm_version: Some("v1".to_string()),
        rag_snapshot_hash: None,
        dataset_version_id: None,
        adapter_ids: Some(vec!["adapter-a".to_string()]),
        base_only: None,
        prompt_text: "hello world".to_string(),
        prompt_truncated: false,
        response_text: Some("hi there".to_string()),
        response_truncated: false,
        rag_doc_ids: None,
        chat_context_hash: None,
        replay_status: Some("available".to_string()),
        latency_ms: Some(12),
        tokens_generated: Some(2),
        determinism_mode: Some("strict".to_string()),
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: None,
        execution_policy_version: None,
        stop_policy_json: None,
        policy_mask_digest_b3: Some("abcd1234".to_string()),
        utf8_healing: None,
    };
    state.db.create_replay_metadata(replay_params).await?;

    // Store a minimal inference trace so envelope can be materialized
    let trace_start = TraceStart {
        trace_id: run_id.clone(),
        tenant_id: tenant_id.clone(),
        request_id: Some(run_id.clone()),
        context_digest: [1u8; 32],
        stack_id: None,
        model_id: None,
        policy_id: None,
    };
    let db_arc = state.db.as_db_arc();
    let mut sink = SqlTraceSink::new(db_arc, trace_start, 1).await?;
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
    let output_tokens = vec![1u32, 2];
    let finalization = TraceFinalization {
        output_tokens: &output_tokens,
        logical_prompt_tokens: 1,
        prefix_cached_token_count: 0,
        billed_input_tokens: 1,
        logical_output_tokens: 2,
        billed_output_tokens: 2,
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
        // Phase 3: Crypto Receipt Dual-Write
        crypto_receipt_digest_b3: None,
        receipt_parity_verified: None,
        tenant_id: None,
        // P0-1: Cache attestation (not needed when prefix_cached_token_count = 0)
        cache_attestation: None,
        worker_public_key: None,
        // UMA telemetry (PRD §5.5)
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
    };
    sink.finalize(finalization).await?;
    sink.flush().await?;

    let response = download_run_evidence(
        State(state.clone()),
        Extension(claims),
        Path(run_id.clone()),
        Query(EvidenceExportParams::default()),
    )
    .await
    .expect("bundle should be returned");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await?;
    let mut archive = ZipArchive::new(std::io::Cursor::new(body))?;
    let mut names = Vec::new();
    for i in 0..archive.len() {
        names.push(archive.by_index(i)?.name().to_string());
    }
    for required in [
        "run_envelope.json",
        "pinned_degradation_evidence.json",
        "replay_metadata.json",
        "policy_digest.json",
        "manifest_ref.json",
        "model_status.json",
        "boot_state.json",
        "README.txt",
    ] {
        assert!(
            names.contains(&required.to_string()),
            "missing {} in archive",
            required
        );
    }

    let mut replay_file = archive.by_name("replay_metadata.json")?;
    let mut replay_contents = String::new();
    replay_file.read_to_string(&mut replay_contents)?;
    assert!(
        replay_contents.contains(&manifest_hash),
        "replay metadata should include manifest hash"
    );

    Ok(())
}

#[tokio::test]
async fn evidence_bundle_not_found_for_unknown_run() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();

    let result = download_run_evidence(
        State(state.clone()),
        Extension(claims),
        Path("missing-run-id".to_string()),
        Query(EvidenceExportParams::default()),
    )
    .await;

    assert!(matches!(result, Err((StatusCode::NOT_FOUND, _))));
    Ok(())
}

#[tokio::test]
async fn evidence_bundle_enforces_tenant_isolation() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;
    let claims = test_admin_claims();
    let run_id = "run-cross-tenant".to_string();

    // Create an alternate tenant and replay metadata scoped to it
    let other_tenant = state.db.create_tenant("Other Tenant", false).await?;
    let sampling_params_json = serde_json::to_string(&SamplingParams::default()).unwrap();
    let replay_params = CreateReplayMetadataParams {
        inference_id: run_id.clone(),
        tenant_id: other_tenant.clone(),
        manifest_hash: "cross-tenant-manifest".to_string(),
        base_model_id: None,
        router_seed: None,
        sampling_params_json,
        backend: "metal".to_string(),
        backend_version: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        sampling_algorithm_version: Some("v1".to_string()),
        rag_snapshot_hash: None,
        dataset_version_id: None,
        adapter_ids: None,
        base_only: None,
        prompt_text: "cross tenant".to_string(),
        prompt_truncated: false,
        response_text: None,
        response_truncated: false,
        rag_doc_ids: None,
        chat_context_hash: None,
        replay_status: Some("available".to_string()),
        latency_ms: None,
        tokens_generated: None,
        determinism_mode: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: None,
        execution_policy_version: None,
        stop_policy_json: None,
        policy_mask_digest_b3: None,
        utf8_healing: None,
    };
    state.db.create_replay_metadata(replay_params).await?;

    let result = download_run_evidence(
        State(state.clone()),
        Extension(claims),
        Path(run_id),
        Query(EvidenceExportParams::default()),
    )
    .await;

    assert!(matches!(result, Err((StatusCode::FORBIDDEN, _))));
    Ok(())
}
