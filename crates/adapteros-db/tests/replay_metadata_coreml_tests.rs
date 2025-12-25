use adapteros_db::replay_metadata::CreateReplayMetadataParams;
use adapteros_db::Db;
use serde::Serialize;
use sqlx::Row;

#[derive(Debug, Serialize)]
struct ReplayCoremlFields {
    package: Option<String>,
    expected: Option<String>,
    mismatch: Option<i64>,
    backend: String,
    determinism_mode: Option<String>,
}

#[tokio::test]
async fn replay_metadata_persists_coreml_hashes() {
    let db = Db::new_in_memory().await.expect("db init");

    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-coreml")
        .bind("CoreML Tenant")
        .execute(db.pool())
        .await
        .expect("insert tenant");

    let coreml_hash = Some("coreml-actual-hash".to_string());
    let coreml_expected = Some("coreml-expected-hash".to_string());

    let params = CreateReplayMetadataParams {
        inference_id: "inf-coreml-1".to_string(),
        tenant_id: "tenant-coreml".to_string(),
        manifest_hash: "manifest-hash".to_string(),
        base_model_id: Some("base-model".to_string()),
        router_seed: Some("router-seed".to_string()),
        sampling_params_json: r#"{"temperature":0.1}"#.to_string(),
        backend: "coreml".to_string(),
        backend_version: Some("v0.1.0".to_string()),
        coreml_package_hash: coreml_hash.clone(),
        coreml_expected_package_hash: coreml_expected.clone(),
        coreml_hash_mismatch: Some(true),
        sampling_algorithm_version: Some("v1.0.0".to_string()),
        rag_snapshot_hash: None,
        dataset_version_id: None,
        adapter_ids: Some(vec!["adapter-1".to_string()]),
        base_only: Some(false),
        prompt_text: "prompt text".to_string(),
        prompt_truncated: false,
        response_text: Some("response text".to_string()),
        response_truncated: false,
        rag_doc_ids: None,
        chat_context_hash: None,
        replay_status: Some("available".to_string()),
        latency_ms: Some(12),
        tokens_generated: Some(3),
        determinism_mode: Some("strict".to_string()),
        fallback_triggered: false,
        coreml_compute_preference: Some("cpu_and_neural_engine".to_string()),
        coreml_compute_units: Some("CpuAndNeuralEngine".to_string()),
        coreml_gpu_used: Some(false),
        fallback_backend: Some("none".to_string()),
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: None,
        execution_policy_version: None,
        stop_policy_json: None,
        policy_mask_digest_b3: None,
    };

    let meta_id = db
        .create_replay_metadata(params)
        .await
        .expect("metadata created");

    let row = sqlx::query(
        r#"
        SELECT
            coreml_package_hash,
            coreml_expected_package_hash,
            coreml_hash_mismatch,
            backend,
            determinism_mode
        FROM inference_replay_metadata
        WHERE id = ?
        "#,
    )
    .bind(meta_id)
    .fetch_one(db.pool())
    .await
    .expect("row loaded");

    let record = ReplayCoremlFields {
        package: row.get::<Option<String>, _>("coreml_package_hash"),
        expected: row.get::<Option<String>, _>("coreml_expected_package_hash"),
        mismatch: row.get::<Option<i64>, _>("coreml_hash_mismatch"),
        backend: row.get::<String, _>("backend"),
        determinism_mode: row.get::<Option<String>, _>("determinism_mode"),
    };

    assert_eq!(record.package, coreml_hash);
    assert_eq!(record.expected, coreml_expected);
    assert_eq!(record.mismatch, Some(1));
    assert_eq!(record.backend, "coreml");
    assert_eq!(record.determinism_mode, Some("strict".to_string()));
}
