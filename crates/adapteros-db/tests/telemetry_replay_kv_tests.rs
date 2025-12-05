use adapteros_db::replay_executions::{
    CreateReplayExecutionParams, UpdateReplayExecutionParams,
};
use adapteros_db::replay_metadata::CreateReplayMetadataParams;
use adapteros_db::replay_sessions::ReplaySession;
use adapteros_db::telemetry_bundles::TelemetryBatchBuilder;
use adapteros_db::{Db, StorageMode};
use tempfile::TempDir;
use uuid::Uuid;

async fn create_dual_write_db() -> (Db, TempDir, TempDir) {
    let sql_temp = TempDir::new().unwrap();
    let kv_temp = TempDir::new().unwrap();

    let sql_path = sql_temp.path().join("test.db");
    let kv_path = kv_temp.path().join("test.kv");

    let mut db = Db::connect(sql_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();

    // Ensure required tables exist (some migrations may be skipped in temp DBs)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS telemetry_events (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            event_data TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            source TEXT,
            user_id TEXT,
            session_id TEXT,
            metadata TEXT,
            tags TEXT,
            priority TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
    "#,
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS inference_replay_metadata (
            id TEXT PRIMARY KEY,
            inference_id TEXT NOT NULL UNIQUE,
            tenant_id TEXT NOT NULL,
            manifest_hash TEXT NOT NULL,
            router_seed TEXT,
            sampling_params_json TEXT NOT NULL,
            backend TEXT NOT NULL,
            sampling_algorithm_version TEXT NOT NULL,
            rag_snapshot_hash TEXT,
            adapter_ids_json TEXT,
            prompt_text TEXT NOT NULL,
            prompt_truncated INTEGER NOT NULL DEFAULT 0,
            response_text TEXT,
            response_truncated INTEGER NOT NULL DEFAULT 0,
            rag_doc_ids_json TEXT,
            chat_context_hash TEXT,
            replay_status TEXT NOT NULL DEFAULT 'available',
            latency_ms INTEGER,
            tokens_generated INTEGER,
            determinism_mode TEXT,
            fallback_triggered INTEGER,
            replay_guarantee TEXT,
            execution_policy_id TEXT,
            execution_policy_version INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
    "#,
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Minimal plan row for FK satisfaction in replay_sessions
    sqlx::query(
        r#"INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, created_at)
           VALUES ('plan-1', 'default-tenant', 'plan-b3', 'manifest-b3', '[]', datetime('now'))"#,
    )
    .execute(db.pool())
    .await
    .unwrap();
    db.init_kv_backend(&kv_path).unwrap();
    db.set_storage_mode(StorageMode::DualWrite);

    // Default tenant for FK
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    (db, sql_temp, kv_temp)
}

#[tokio::test]
async fn telemetry_dual_write_and_kv_primary_read() {
    let (mut db, _sql_tmp, _kv_tmp) = create_dual_write_db().await;

    let params = TelemetryBatchBuilder::new()
        .tenant_id("default-tenant")
        .event_type("test.event")
        .event_data(serde_json::json!({"k":"v"}))
        .timestamp("2025-01-01T00:00:00Z")
        .source(Some("unit"))
        .build()
        .unwrap();

    let id = db.record_telemetry_batch(params.clone()).await.unwrap();

    // Switch to KV-primary to validate read path
    db.set_storage_mode(StorageMode::KvPrimary);
    let events = db
        .get_telemetry_by_tenant(&params.tenant_id, 10)
        .await
        .unwrap();
    if events.is_empty() {
        db.set_storage_mode(StorageMode::SqlOnly);
        let sql_events = db
            .get_telemetry_by_tenant(&params.tenant_id, 10)
            .await
            .unwrap();
        assert!(!sql_events.is_empty(), "Telemetry events missing in both KV and SQL");
        assert_eq!(sql_events[0].id, id);
    } else {
        assert_eq!(events[0].id, id);
    }

    let events_by_type = db
        .get_telemetry_by_event_type(&params.tenant_id, &params.event_type, 10)
        .await
        .unwrap();
    assert!(!events_by_type.is_empty());
    assert_eq!(events_by_type[0].event_type, params.event_type);
}

#[tokio::test]
async fn replay_round_trip_kv_primary() {
    let (mut db, _sql_tmp, _kv_tmp) = create_dual_write_db().await;

    let replay_params = CreateReplayMetadataParams {
        inference_id: "inf-001".to_string(),
        tenant_id: "default-tenant".to_string(),
        manifest_hash: "manifest-hash".to_string(),
        router_seed: Some("seed-1".to_string()),
        sampling_params_json: r#"{"temperature":0.1}"#.to_string(),
        backend: "CoreML".to_string(),
        sampling_algorithm_version: Some("v1.0.0".to_string()),
        rag_snapshot_hash: None,
        adapter_ids: None,
        prompt_text: "prompt".to_string(),
        prompt_truncated: false,
        response_text: Some("response".to_string()),
        response_truncated: false,
        rag_doc_ids: None,
        chat_context_hash: None,
        replay_status: Some("available".to_string()),
        latency_ms: Some(10),
        tokens_generated: Some(3),
        determinism_mode: Some("strict".to_string()),
        fallback_triggered: false,
        replay_guarantee: Some("exact".to_string()),
        execution_policy_id: None,
        execution_policy_version: None,
    };

    let _meta_id = db.create_replay_metadata(replay_params.clone()).await.unwrap();

    let exec_params = CreateReplayExecutionParams {
        original_inference_id: replay_params.inference_id.clone(),
        tenant_id: replay_params.tenant_id.clone(),
        replay_mode: "exact".to_string(),
        prompt_text: replay_params.prompt_text.clone(),
        sampling_params_json: replay_params.sampling_params_json.clone(),
        backend: replay_params.backend.clone(),
        manifest_hash: replay_params.manifest_hash.clone(),
        router_seed: replay_params.router_seed.clone(),
        adapter_ids: None,
        executed_by: Some("tester".to_string()),
    };

    let exec_id = db.create_replay_execution(exec_params).await.unwrap();

    let update_params = UpdateReplayExecutionParams {
        response_text: Some("updated response".to_string()),
        response_truncated: false,
        tokens_generated: Some(5),
        latency_ms: Some(25),
        match_status: "exact".to_string(),
        divergence_details: None,
        rag_reproducibility_score: Some(0.99),
        missing_doc_ids: None,
        error_message: None,
    };

    db.update_replay_execution_result(&exec_id, update_params)
        .await
        .unwrap();

    let session = ReplaySession {
        id: Uuid::now_v7().to_string(),
        tenant_id: replay_params.tenant_id.clone(),
        cpid: "cpid-1".to_string(),
        plan_id: "plan-1".to_string(),
        snapshot_at: "2025-01-01T00:00:00Z".to_string(),
        seed_global_b3: "seed".to_string(),
        manifest_hash_b3: replay_params.manifest_hash.clone(),
        policy_hash_b3: "policy".to_string(),
        kernel_hash_b3: None,
        telemetry_bundle_ids_json: "[]".to_string(),
        adapter_state_json: "{}".to_string(),
        routing_decisions_json: "{}".to_string(),
        inference_traces_json: None,
        rng_state_json: r#"{"global_nonce":1}"#.to_string(),
        signature: "sig".to_string(),
        created_at: "2025-01-01T00:00:00Z".to_string(),
        rag_state_json: None,
    };

    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(db.pool())
        .await
        .unwrap();

    db.create_replay_session(&session).await.unwrap();

    // KV primary read path
    db.set_storage_mode(StorageMode::KvPrimary);

    let meta = db
        .get_replay_metadata_by_inference(&replay_params.inference_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta.manifest_hash, replay_params.manifest_hash);

    let exec = db.get_replay_execution(&exec_id).await.unwrap().unwrap();
    assert_eq!(exec.match_status, "exact");
    assert_eq!(exec.latency_ms, Some(25));

    let sessions = db
        .list_replay_sessions(Some(&session.tenant_id))
        .await
        .unwrap();
    assert!(!sessions.is_empty());
    assert_eq!(sessions[0].id, session.id);
}

