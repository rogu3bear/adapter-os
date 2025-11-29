use adapteros_db::users::Role;
use adapteros_server_api::types::{ErrorResponse, LoginRequest, LoginResponse};
use adapteros_server_api::AppState;
use adapteros_testing::{TestAppStateBuilder, TestAuth, TestDbBuilder, TestUser};
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

/// Create test app state using consolidated fixtures
pub async fn create_test_app_state() -> AppState {
    let _ = std::fs::create_dir_all("var/bundles");

    // Use consolidated TestDbBuilder and TestAppStateBuilder
    let db = TestDbBuilder::new()
        .with_tenant(DEFAULT_TENANT_ID, DEFAULT_TENANT_NAME)
        .with_user(TestUser {
            email: DEFAULT_USER_EMAIL.to_string(),
            display_name: DEFAULT_USER_DISPLAY_NAME.to_string(),
            password: DEFAULT_USER_PASSWORD.to_string(),
            role: Role::Admin,
            tenant_id: DEFAULT_TENANT_ID.to_string(),
        })
        .build()
        .await
        .expect("failed to build test database");

    let state = TestAppStateBuilder::new()
        .with_db(db)
        .with_jwt_secret(DEFAULT_JWT_SECRET.to_vec())
        .build()
        .await
        .expect("failed to build test app state");

    // Register default metric series
    for name in DEFAULT_METRIC_SERIES {
        state
            .metrics_registry()
            .get_or_create_series(name.to_string(), 1_000, 1_024);
    }

    state
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
