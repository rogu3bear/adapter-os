use adapteros_config::test_support::TestEnvGuard;
use adapteros_db::{CreateReplayMetadataParams, Db};

#[tokio::test]
async fn stores_ciphertext_and_recovers_plaintext() {
    let _guard = TestEnvGuard::new();

    // Enable crypto-at-rest with local fallback to avoid UDS dependency in tests.
    std::env::set_var("AOS_CRYPTO_AT_REST", "1");
    std::env::set_var("AOS_CRYPTO_FAKE", "1");

    let db = Db::new_in_memory().await.expect("Failed to create in-memory database for replay metadata crypto test");
    let tenant_id = db
        .create_tenant("Crypto Tenant", false)
        .await
        .expect("Failed to create tenant for replay metadata crypto test");

    let params = CreateReplayMetadataParams {
        inference_id: "crypto-inference-1".to_string(),
        tenant_id: tenant_id.clone(),
        manifest_hash: "manifest-hash".to_string(),
        base_model_id: None,
        router_seed: None,
        sampling_params_json: r#"{"temperature":0.1}"#.to_string(),
        backend: "CoreML".to_string(),
        backend_version: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        sampling_algorithm_version: None,
        rag_snapshot_hash: None,
        dataset_version_id: None,
        adapter_ids: None,
        base_only: None,
        prompt_text: "secret prompt text".to_string(),
        prompt_truncated: false,
        response_text: Some("secret response text".to_string()),
        response_truncated: false,
        rag_doc_ids: None,
        chat_context_hash: None,
        replay_status: None,
        latency_ms: None,
        tokens_generated: None,
        determinism_mode: None,
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        replay_guarantee: None,
        execution_policy_id: None,
        execution_policy_version: None,
        stop_policy_json: None,
        policy_mask_digest_b3: None,
        utf8_healing: None,
    };

    let id = db
        .create_replay_metadata(params)
        .await
        .expect("create metadata");

    // Raw DB storage should contain ciphertext JSON, not plaintext.
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT prompt_text, response_text FROM inference_replay_metadata WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(db.pool())
    .await
    .expect("fetch row");

    assert!(
        row.0.contains("ciphertext_b64"),
        "prompt_text should be stored as encrypted JSON"
    );
    assert!(
        !row.0.contains("secret prompt text"),
        "prompt_text should not store plaintext"
    );
    if let Some(resp) = row.1 {
        assert!(
            resp.contains("ciphertext_b64"),
            "response_text should be encrypted JSON"
        );
        assert!(
            !resp.contains("secret response text"),
            "response_text should not store plaintext"
        );
    }

    // API helpers should decrypt back to plaintext on read.
    let metadata = db
        .get_replay_metadata(&id)
        .await
        .expect("get metadata")
        .expect("metadata present");

    assert_eq!(metadata.prompt_text, "secret prompt text");
    assert_eq!(
        metadata.response_text.as_deref(),
        Some("secret response text")
    );
    // Guard automatically restores env vars on drop
}
