//! Integration test for model UI user journey
use adapteros_db::Db;
use adapteros_server_api::{
    auth::Claims,
    handlers::models::{self, ImportModelRequest},
    state::AppState,
};
use axum::{extract::State, http::StatusCode, Extension, Json};
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_env() -> anyhow::Result<(AppState, Claims)> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let app_state = AppState::new(db, Default::default());

    let tenant_id = "test-tenant-e2e".to_string();
    let user_id = "test-user-e2e".to_string();

    // Ensure tenant exists
    sqlx::query!(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)",
        tenant_id,
        "Test Tenant E2E"
    )
    .execute(app_state.db.pool())
    .await?;

    let claims = Claims {
        sub: user_id,
        tenant_id,
        role: "admin".to_string(),
        exp: 0,
    };
    Ok((app_state, claims))
}

#[tokio::test]
async fn test_model_ui_journey_e2e() -> anyhow::Result<()> {
    let (state, claims) = setup_test_env().await?;
    let model_id = Uuid::new_v4().to_string();

    // Mock a base model status entry to load/unload
    sqlx::query!(
        "INSERT INTO base_model_status (model_id, tenant_id, model_name, status, is_loaded) VALUES (?, ?, ?, ?, ?)",
        model_id,
        claims.tenant_id,
        "qwen2.5-7b-e2e",
        "unloaded",
        false
    )
    .execute(state.db.pool())
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

    let import_record = sqlx::query!(
        "SELECT status FROM base_model_imports WHERE id = ?",
        import_id
    )
    .fetch_one(state.db.pool())
    .await?;
    assert_eq!(import_record.status, "validating");

    // --- Step 2: Load the model ---
    let load_res = models::load_model(State(state.clone()), Extension(claims.clone()), axum::extract::Path(model_id.clone())).await;
    assert!(load_res.is_ok(), "Load model should succeed");

    let status_record = sqlx::query!("SELECT status, is_loaded FROM base_model_status WHERE model_id = ?", model_id)
        .fetch_one(state.db.pool())
        .await?;
    assert_eq!(status_record.status, "loaded");
    assert!(status_record.is_loaded > 0);

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
    
    let final_status_record = sqlx::query!("SELECT status, is_loaded FROM base_model_status WHERE model_id = ?", model_id)
        .fetch_one(state.db.pool())
        .await?;
    assert_eq!(final_status_record.status, "unloaded");
    assert!(final_status_record.is_loaded == 0);
    
    // --- Step 5: Verify Journey Tracking ---
    let journey_steps = sqlx::query!(
        "SELECT step_completed FROM onboarding_journeys WHERE tenant_id = ? AND user_id = ? ORDER BY completed_at",
        claims.tenant_id,
        claims.sub
    )
    .fetch_all(state.db.pool())
    .await?;
    
    assert_eq!(journey_steps.len(), 1, "Should have one journey step for model_loaded");
    assert_eq!(journey_steps[0].step_completed, "model_loaded");
    
    // Cleanup dummy files
    tokio::fs::remove_dir_all("testdata").await?;

    Ok(())
}

