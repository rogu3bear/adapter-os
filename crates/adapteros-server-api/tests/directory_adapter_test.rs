use adapteros_db::Db;
use adapteros_server_api::handlers::{upsert_directory_adapter, DirectoryUpsertRequest};
use adapteros_server_api::state::{ApiConfig, AppState, CryptoState, MetricsConfig};
use axum::extract::{Extension, Json, State};
use axum::http::StatusCode;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tempfile::TempDir;
use tokio::sync::Mutex;

// Mock Claims for testing
#[derive(Clone)]
struct MockClaims {
    role: String,
    user_id: String,
}

#[tokio::test]
async fn test_directory_adapter_normal_operation() {
    // Create a temporary directory with some test files
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("test_project");
    std::fs::create_dir(&test_dir).unwrap();
    std::fs::write(test_dir.join("main.rs"), "fn main() {}").unwrap();
    std::fs::write(test_dir.join("lib.rs"), "pub fn hello() {}").unwrap();

    // Set up test state
    let db = Db::new_temp().await.unwrap();
    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
    };

    let crypto = Arc::new(CryptoState::new());
    let state = AppState {
        db: db.clone(),
        jwt_secret: Arc::new(vec![1, 2, 3]),
        config: Arc::new(RwLock::new(config)),
        metrics_exporter: Arc::new(adapteros_metrics_exporter::MetricsExporter::new()),
        training_service: Arc::new(adapteros_orchestrator::TrainingService::new()),
        git_subsystem: None,
        file_change_tx: None,
        crypto,
        lifecycle_manager: None,
        code_job_manager: None,
        worker: None,
        active_stack: Arc::new(RwLock::new(None)),
        db_pool: db.pool.clone(),
    };

    // Create request
    let request = DirectoryUpsertRequest {
        root: test_dir.to_string_lossy().to_string(),
        path: ".".to_string(),
        tenant_id: "test-tenant".to_string(),
        activate: false,
    };

    // Mock claims (Admin role should have access)
    let claims = adapteros_server_api::Claims {
        sub: "test@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
    };

    // Execute handler
    let result = upsert_directory_adapter(State(state), Extension(claims), Json(request)).await;

    // Verify success
    assert!(result.is_ok(), "Handler should succeed for valid directory");
    let (status, response) = result.unwrap();
    assert_eq!(status, StatusCode::CREATED);
    assert!(response
        .0
        .adapter_id
        .starts_with("directory::test-tenant::"));
}

#[tokio::test]
async fn test_directory_adapter_invalid_path() {
    let db = Db::new_temp().await.unwrap();
    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
    };

    let crypto = Arc::new(CryptoState::new());
    let state = AppState {
        db: db.clone(),
        jwt_secret: Arc::new(vec![1, 2, 3]),
        config: Arc::new(RwLock::new(config)),
        metrics_exporter: Arc::new(adapteros_metrics_exporter::MetricsExporter::new()),
        training_service: Arc::new(adapteros_orchestrator::TrainingService::new()),
        git_subsystem: None,
        file_change_tx: None,
        crypto,
        lifecycle_manager: None,
        code_job_manager: None,
        worker: None,
        active_stack: Arc::new(RwLock::new(None)),
        db_pool: db.pool.clone(),
    };

    // Test with parent directory traversal
    let request = DirectoryUpsertRequest {
        root: "/tmp".to_string(),
        path: "../etc".to_string(), // Should be rejected
        tenant_id: "test-tenant".to_string(),
        activate: false,
    };

    let claims = adapteros_server_api::Claims {
        sub: "test@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
    };

    let result = upsert_directory_adapter(State(state), Extension(claims), Json(request)).await;

    // Verify rejection
    assert!(result.is_err(), "Handler should reject path with '..'");
    let (status, _error) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_directory_adapter_nonexistent_root() {
    let db = Db::new_temp().await.unwrap();
    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
    };

    let crypto = Arc::new(CryptoState::new());
    let state = AppState {
        db: db.clone(),
        jwt_secret: Arc::new(vec![1, 2, 3]),
        config: Arc::new(RwLock::new(config)),
        metrics_exporter: Arc::new(adapteros_metrics_exporter::MetricsExporter::new()),
        training_service: Arc::new(adapteros_orchestrator::TrainingService::new()),
        git_subsystem: None,
        file_change_tx: None,
        crypto,
        lifecycle_manager: None,
        code_job_manager: None,
        worker: None,
        active_stack: Arc::new(RwLock::new(None)),
        db_pool: db.pool.clone(),
    };

    let request = DirectoryUpsertRequest {
        root: "/nonexistent/path/12345".to_string(),
        path: ".".to_string(),
        tenant_id: "test-tenant".to_string(),
        activate: false,
    };

    let claims = adapteros_server_api::Claims {
        sub: "test@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
    };

    let result = upsert_directory_adapter(State(state), Extension(claims), Json(request)).await;

    // Verify rejection
    assert!(result.is_err(), "Handler should reject nonexistent root");
    let (status, _error) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_directory_adapter_configurable_timeout() {
    let db = Db::new_temp().await.unwrap();

    // Test with custom timeout
    let config = ApiConfig {
        metrics: MetricsConfig {
            enabled: false,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 5, // Very short timeout
    };

    let crypto = Arc::new(CryptoState::new());
    let state = AppState {
        db: db.clone(),
        jwt_secret: Arc::new(vec![1, 2, 3]),
        config: Arc::new(RwLock::new(config)),
        metrics_exporter: Arc::new(adapteros_metrics_exporter::MetricsExporter::new()),
        training_service: Arc::new(adapteros_orchestrator::TrainingService::new()),
        git_subsystem: None,
        file_change_tx: None,
        crypto,
        lifecycle_manager: None,
        code_job_manager: None,
        worker: None,
        active_stack: Arc::new(RwLock::new(None)),
        db_pool: db.pool.clone(),
    };

    // Create a simple directory that should analyze quickly
    let temp_dir = TempDir::new().unwrap();
    let test_dir = temp_dir.path().join("small");
    std::fs::create_dir(&test_dir).unwrap();
    std::fs::write(test_dir.join("test.txt"), "test").unwrap();

    let request = DirectoryUpsertRequest {
        root: test_dir.to_string_lossy().to_string(),
        path: ".".to_string(),
        tenant_id: "test-tenant".to_string(),
        activate: false,
    };

    let claims = adapteros_server_api::Claims {
        sub: "test@example.com".to_string(),
        role: "admin".to_string(),
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
    };

    // Should succeed even with short timeout because directory is small
    let result = upsert_directory_adapter(State(state), Extension(claims), Json(request)).await;

    assert!(
        result.is_ok(),
        "Handler should succeed with custom timeout for small directory"
    );
}
