#![cfg(all(test, feature = "extended-tests"))]

//! Integration test for model UI user journey
use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::Db;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::{
    auth::Claims,
    config::PathsConfig,
    handlers::models::{self, ImportModelRequest},
    state::{ApiConfig, AppState, MetricsConfig},
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

async fn setup_test_env() -> anyhow::Result<(AppState, Claims)> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let api_config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: String::new(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: false,
        capacity_limits: Default::default(),
        general: None,
        server: Default::default(),
        security: Default::default(),
        auth: Default::default(),
        performance: Default::default(),
        paths: PathsConfig {
            artifacts_root: "var/artifacts".to_string(),
            bundles_root: "var/bundles".to_string(),
            adapters_root: "var/adapters/repo".to_string(),
            plan_dir: "var/plan".to_string(),
            datasets_root: "var/datasets".to_string(),
            documents_root: "var/documents".to_string(),
            synthesis_model_path: None,
            training_worker_bin: None,
        },
        chat_context: Default::default(),
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Auto,
        worker_id: 0,
        self_hosting: Default::default(),
        streaming: Default::default(),
        timeouts: Default::default(),
        rate_limit: None,
    }));
    let metrics_exporter = Arc::new(MetricsExporter::new(vec![0.1, 0.5, 1.0])?);
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new()?);
    let metrics_registry =
        Arc::new(adapteros_telemetry::MetricsRegistry::new(metrics_collector.clone()));
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }
    let training_service = Arc::new(TrainingService::new());
    let app_state = AppState::with_sqlite(
        db,
        b"test-secret".to_vec(),
        api_config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        training_service,
    );

    let tenant_id = "test-tenant-e2e".to_string();
    let user_id = "test-user-e2e".to_string();

    // Ensure tenant exists
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind("Test Tenant E2E")
        .execute(app_state.db.pool_result().unwrap())
        .await?;

    let claims = Claims {
        sub: user_id,
        tenant_id,
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        exp: 0,
    };
    Ok((app_state, claims))
}

#[tokio::test]
async fn test_model_ui_journey_e2e() -> anyhow::Result<()> {
    let (state, claims) = setup_test_env().await?;
    let model_id = Uuid::new_v4().to_string();

    // Mock a base model status entry to load/unload
    sqlx::query(
        "INSERT INTO base_model_status (model_id, tenant_id, model_name, status, is_loaded) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&model_id)
    .bind(&claims.tenant_id)
    .bind("qwen2.5-7b-e2e")
    .bind("unloaded")
    .bind(false)
    .execute(state.db.pool_result().unwrap())
    .await?;

    // --- Step 1: Import a new base model ---
    let import_req = Json(ImportModelRequest {
        model_name: "test-model-e2e".to_string(),
        weights_path: "testdata/model.safetensors".to_string(),
        config_path: "testdata/config.json".to_string(),
        tokenizer_path: "testdata/tokenizer.json".to_string(),
        tokenizer_config_path: None,
        metadata: None,
    });
    
    // Create dummy files to pass path validation
    tokio::fs::create_dir_all("testdata").await?;
    tokio::fs::write("testdata/model.safetensors", "").await?;
    tokio::fs::write("testdata/config.json", "").await?;
    tokio::fs::write("testdata/tokenizer.json", "").await?;

    let import_res = models::import_model(State(state.clone()), Extension(claims.clone()), import_req).await;
    assert!(import_res.is_ok(), "Import model should succeed");
    let import_id = import_res.unwrap().0.import_id;

    let import_status: String =
        sqlx::query_scalar("SELECT status FROM base_model_imports WHERE id = ?")
            .bind(import_id)
            .fetch_one(state.db.pool_result().unwrap())
            .await?;
    assert_eq!(import_status, "validating");

    // --- Step 2: Load the model ---
    let load_res = models::load_model(State(state.clone()), Extension(claims.clone()), axum::extract::Path(model_id.clone())).await;
    assert!(load_res.is_ok(), "Load model should succeed");

    let (status, is_loaded): (String, i64) =
        sqlx::query_as("SELECT status, is_loaded FROM base_model_status WHERE model_id = ?")
            .bind(&model_id)
            .fetch_one(state.db.pool_result().unwrap())
            .await?;
    assert_eq!(status, "loaded");
    assert!(is_loaded > 0);

    // --- Step 3: Get Cursor Config ---
    let config_res = models::get_cursor_config(State(state.clone()), Extension(claims.clone())).await;
    assert!(config_res.is_ok(), "Get cursor config should succeed");
    let config = config_res.unwrap().0;
    assert!(config.is_ready);
    assert_eq!(config.model_id, model_id);

    // --- Step 4: Unload the model ---
    let unload_res = models::unload_model(State(state.clone()), Extension(claims.clone()), axum::extract::Path(model_id.clone())).await;
    assert!(unload_res.is_ok(), "Unload model should succeed");
    assert_eq!(unload_res.unwrap(), StatusCode::OK);
    
    let (final_status, final_is_loaded): (String, i64) =
        sqlx::query_as("SELECT status, is_loaded FROM base_model_status WHERE model_id = ?")
            .bind(&model_id)
            .fetch_one(state.db.pool_result().unwrap())
            .await?;
    assert_eq!(final_status, "unloaded");
    assert!(final_is_loaded == 0);
    
    // --- Step 5: Verify Journey Tracking ---
    let journey_steps: Vec<String> = sqlx::query_scalar(
        "SELECT step_completed FROM onboarding_journeys WHERE tenant_id = ? AND user_id = ? ORDER BY completed_at",
    )
    .bind(&claims.tenant_id)
    .bind(&claims.sub)
    .fetch_all(state.db.pool_result().unwrap())
    .await?;
    
    assert_eq!(journey_steps.len(), 1, "Should have one journey step for model_loaded");
    assert_eq!(journey_steps[0], "model_loaded");
    
    // Cleanup dummy files
    tokio::fs::remove_dir_all("testdata").await?;

    Ok(())
}
