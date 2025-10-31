use std::sync::{Arc, RwLock};

use adapteros_db::{users::Role, Db};
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::auth::hash_password;
use adapteros_server_api::state::{ApiConfig, MetricsConfig};
use adapteros_server_api::types::{ErrorResponse, LoginRequest, LoginResponse};
use adapteros_server_api::AppState;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt;

pub const DEFAULT_TENANT_ID: &str = "default";
pub const DEFAULT_TENANT_NAME: &str = "Default Tenant";
pub const DEFAULT_USER_EMAIL: &str = "admin@example.com";
pub const DEFAULT_USER_DISPLAY_NAME: &str = "Test Admin";
pub const DEFAULT_USER_PASSWORD: &str = "correct-horse-battery-staple";
pub const DEFAULT_JWT_SECRET: &[u8] = b"test-auth-secret";
pub const DEFAULT_METRIC_SERIES: [&str; 4] = [
    "inference_latency_p95_ms",
    "queue_depth",
    "tokens_per_second",
    "memory_usage_mb",
];

fn default_api_config() -> ApiConfig {
    ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test-token".to_string(),
            system_metrics_interval_secs: 30,
        },
        golden_gate: None,
        bundles_root: "test-bundles".to_string(),
        rate_limits: None,
    }
}

pub async fn create_test_app_state() -> AppState {
    let db = Db::connect("sqlite::memory:")
        .await
        .expect("failed to connect to in-memory sqlite");
    db.migrate()
        .await
        .expect("failed to run database migrations for tests");

    let _ = std::fs::create_dir_all("var/bundles");

    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
        .bind(DEFAULT_TENANT_ID)
        .bind(DEFAULT_TENANT_NAME)
        .bind(0)
        .execute(db.pool())
        .await
        .expect("failed to seed test tenant");

    let password_hash =
        hash_password(DEFAULT_USER_PASSWORD).expect("failed to hash default test password");
    db.create_user(
        DEFAULT_USER_EMAIL,
        DEFAULT_USER_DISPLAY_NAME,
        &password_hash,
        Role::Admin,
    )
    .await
    .expect("failed to seed test user");

    let api_config = Arc::new(RwLock::new(default_api_config()));

    let metrics_exporter = Arc::new(
        MetricsExporter::new(vec![0.1, 0.5, 1.0]).expect("failed to create metrics exporter"),
    );
    let metrics_collector = Arc::new(
        adapteros_telemetry::MetricsCollector::new().expect("failed to create metrics collector"),
    );
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));
    for name in DEFAULT_METRIC_SERIES {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }
    let training_service = Arc::new(TrainingService::new());

    AppState::with_sqlite(
        db,
        DEFAULT_JWT_SECRET.to_vec(),
        api_config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        training_service,
    )
}

pub async fn login_user(
    app: &Router<AppState>,
    email: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let payload = LoginRequest {
        email: email.to_string(),
        password: password.to_string(),
    };

    let request = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&payload).map_err(|e| format!("serialize login payload: {e}"))?,
        ))
        .map_err(|e| format!("build login request: {e}"))?;

    let response = app
        .clone()
        .oneshot(request)
        .await
        .map_err(|e| format!("execute login request: {e}"))?;

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|e| format!("read login response body: {e}"))?;

    if status != StatusCode::OK {
        if let Ok(err) = serde_json::from_slice::<ErrorResponse>(&bytes) {
            return Err(format!(
                "login failed with {} ({}): {}",
                status, err.code, err.error
            ));
        }
        return Err(format!(
            "login failed with {} and body: {}",
            status,
            String::from_utf8_lossy(&bytes)
        ));
    }

    let response: LoginResponse =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse login response: {e}"))?;
    Ok(response)
}

pub async fn make_authenticated_request(
    app: &Router<AppState>,
    path: &str,
    token: &str,
) -> Result<String, String> {
    let request = Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .map_err(|e| format!("build authenticated request: {e}"))?;

    let response = app
        .clone()
        .oneshot(request)
        .await
        .map_err(|e| format!("execute authenticated request: {e}"))?;

    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|e| format!("read response body: {e}"))?;

    if !status.is_success() {
        if let Ok(err) = serde_json::from_slice::<ErrorResponse>(&bytes) {
            return Err(format!(
                "request to {} failed with {} ({}): {}",
                path, status, err.code, err.error
            ));
        }
        return Err(format!(
            "request to {} failed with {} and body: {}",
            path,
            status,
            String::from_utf8_lossy(&bytes)
        ));
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
