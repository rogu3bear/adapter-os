use adapteros_server_api::handlers::models::load_model;
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tempfile::TempDir;

mod common;

#[tokio::test]
async fn load_model_returns_404_when_model_path_missing() {
    let _env_guard = common::env_lock().await;
    // Use a temp directory to avoid polluting the var/ directory and ensure test isolation.
    let temp_dir = TempDir::with_prefix("aos-test-socket-").expect("create temp socket dir");
    let fake_socket = temp_dir.path().join("nonexistent.sock");
    std::fs::write(&fake_socket, b"").expect("touch fake socket");
    std::env::set_var("AOS_WORKER_SOCKET", fake_socket.to_str().unwrap());

    let state: AppState = common::setup_state(None).await.expect("setup state");
    let claims = common::test_admin_claims();

    // Insert a model pointing to a missing path.
    let model_id = "model-missing-path";
    let missing_path = "var/definitely-not-here/model";
    adapteros_db::sqlx::query(
        r#"
        INSERT INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, backend, model_path, tenant_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(model_id)
    .bind("Test Model")
    .bind("hash")
    .bind("cfg_hash")
    .bind("tok_hash")
    .bind("tok_cfg_hash")
    .bind("mlx")
    .bind(missing_path)
    .bind(&claims.tenant_id)
    .execute(state.db.pool())
    .await
    .expect("insert model");

    let err = match load_model(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_id.to_string()),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("expected path-missing error"),
    };
    assert_eq!(err.status, StatusCode::NOT_FOUND);
    assert_eq!(err.code, "MODEL_PATH_MISSING");

    // Verify status persisted as error with message.
    let status_row = state
        .db
        .get_base_model_status(&claims.tenant_id)
        .await
        .expect("status query")
        .expect("status row");
    assert_eq!(status_row.status, "error");
    let error_message = status_row.error_message.clone().unwrap_or_default();
    assert!(
        error_message.contains("model path does not exist"),
        "unexpected error message: {:?}",
        status_row.error_message
    );

    // temp_dir auto-cleans on drop
    std::env::remove_var("AOS_WORKER_SOCKET");
}

#[tokio::test]
async fn load_model_rejects_path_outside_allowed_root() {
    let _env_guard = common::env_lock().await;
    // Use a temp directory to avoid polluting the var/ directory and ensure test isolation.
    let temp_socket_dir = TempDir::with_prefix("aos-test-socket-").expect("create temp socket dir");
    let fake_socket = temp_socket_dir.path().join("nonexistent.sock");
    std::fs::write(&fake_socket, b"").expect("touch fake socket");
    std::env::set_var("AOS_WORKER_SOCKET", fake_socket.to_str().unwrap());

    let allowed_root = TempDir::with_prefix("aos-test-allowed-").expect("allowed root");
    let disallowed_root = TempDir::with_prefix("aos-test-disallowed-").expect("disallowed root");
    std::env::set_var(
        "AOS_MODEL_CACHE_DIR",
        allowed_root.path().to_string_lossy().to_string(),
    );

    let model_dir = disallowed_root.path().join("model");
    std::fs::create_dir_all(&model_dir).expect("create model dir");
    std::fs::write(model_dir.join("config.json"), "{}").expect("write config");
    std::fs::write(model_dir.join("tokenizer.json"), "{}").expect("write tokenizer");
    std::fs::write(model_dir.join("model.safetensors"), b"stub").expect("write weights");

    let state: AppState = common::setup_state(None).await.expect("setup state");
    let claims = common::test_admin_claims();

    let model_id = "model-outside-root";
    adapteros_db::sqlx::query(
        r#"
        INSERT INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, backend, model_path, tenant_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(model_id)
    .bind("Test Model")
    .bind("hash")
    .bind("cfg_hash")
    .bind("tok_hash")
    .bind("tok_cfg_hash")
    .bind("mlx")
    .bind(model_dir.to_string_lossy().to_string())
    .bind(&claims.tenant_id)
    .execute(state.db.pool())
    .await
    .expect("insert model");

    let err = match load_model(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(model_id.to_string()),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => panic!("expected forbidden error"),
    };

    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "MODEL_PATH_FORBIDDEN");

    // temp_socket_dir auto-cleans on drop
    std::env::remove_var("AOS_MODEL_CACHE_DIR");
    std::env::remove_var("AOS_WORKER_SOCKET");
}
