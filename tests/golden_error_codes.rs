#![cfg(all(test, feature = "extended-tests"))]

//! Golden tests for error code consistency
//!
//! These tests ensure that error codes are consistent across all error scenarios:
//! - 401 responses always use UNAUTHORIZED code
//! - 403 responses always use FORBIDDEN code
//! - Database errors always use DATABASE_ERROR code
//! - Rate limit errors include Retry-After header and details.retryAfter

#[path = "common/mod.rs"]
mod common;

use adapteros_server_api::routes;
use adapteros_server_api::state::{ApiConfig, CorsConfig, MetricsConfig, RateLimitApiConfig};
use adapteros_server_api::types::ErrorResponse;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::auth::{create_test_app_state, login_user, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD};
use tower::ServiceExt;

#[tokio::test]
async fn test_401_error_codes() {
    // Test that all 401 responses use UNAUTHORIZED code
    let state = create_test_app_state().await;
    let app = routes::build(state);

    // Test 1: Missing authorization header
    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .body(Body::empty())
        .expect("build request without auth");

    let response = app.clone().oneshot(request).await.expect("execute request");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Missing auth should return 401"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read error body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("error should deserialize");
    assert_eq!(
        error.code, "UNAUTHORIZED",
        "401 response should have UNAUTHORIZED code, got: {}",
        error.code
    );

    // Test 2: Invalid token
    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", "Bearer invalid-token")
        .body(Body::empty())
        .expect("build request with invalid token");

    let response = app.clone().oneshot(request).await.expect("execute request");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid token should return 401"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read error body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("error should deserialize");
    assert_eq!(
        error.code, "UNAUTHORIZED",
        "401 response should have UNAUTHORIZED code, got: {}",
        error.code
    );
}

#[tokio::test]
async fn test_403_error_codes() {
    // Test that all 403 responses use FORBIDDEN code
    let state = create_test_app_state().await;
    let app = routes::build(state);

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    // Test: User role accessing admin endpoint
    use adapteros_db::users::Role;
    use adapteros_server_api::auth::Claims;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let iat = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let exp = iat + 3600;

    let user_claims = Claims {
        sub: "test_user".to_string(),
        role: Role::User.to_string(),
        iat: iat as usize,
        exp: exp as usize,
        email: "user@test.com".to_string(),
        tenant_id: "default".to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
        nbf: iat as usize,
    };

    let user_token = encode(
        &Header::default(),
        &user_claims,
        &EncodingKey::from_secret(b"test_jwt_secret_key_for_integration_tests_only"),
    )
    .unwrap();

    let request = Request::builder()
        .method("GET")
        .uri("/v1/users")
        .header("authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .expect("build request with user role");

    let response = app.clone().oneshot(request).await.expect("execute request");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "Insufficient permissions should return 403"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read error body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("error should deserialize");
    assert_eq!(
        error.code, "FORBIDDEN",
        "403 response should have FORBIDDEN code, got: {}",
        error.code
    );
}

#[tokio::test]
async fn test_429_retry_after_header() {
    // Test that 429 responses include Retry-After header and details.retryAfter
    let state = create_test_app_state().await;
    {
        let mut config = state
            .config
            .write()
            .expect("config lock should not be poisoned");
        config.rate_limits = Some(RateLimitApiConfig {
            requests_per_minute: 1,
            burst_size: 0,
        });
    }

    let app = routes::build(state.clone());

    let login = login_user(&app, DEFAULT_USER_EMAIL, DEFAULT_USER_PASSWORD)
        .await
        .expect("login should succeed");

    // First request should succeed
    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", format!("Bearer {}", login.token))
        .body(Body::empty())
        .expect("build first request");

    let _response = app
        .clone()
        .oneshot(request)
        .await
        .expect("execute first request");

    // Second request should hit rate limit
    let request = Request::builder()
        .method("GET")
        .uri("/v1/tenants")
        .header("authorization", format!("Bearer {}", login.token))
        .body(Body::empty())
        .expect("build rate-limited request");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("execute rate-limited request");

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Rate limit should return 429"
    );

    // Verify Retry-After header
    let retry_after_header = response.headers().get("retry-after");
    assert!(
        retry_after_header.is_some(),
        "429 response should include Retry-After header"
    );
    let retry_after_seconds: u64 = retry_after_header
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .expect("Retry-After should be a number");
    assert!(
        retry_after_seconds >= 1,
        "Retry-After should be at least 1 second"
    );

    // Verify error code and details.retryAfter
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read error body");
    let error: ErrorResponse =
        serde_json::from_slice(&body).expect("error should deserialize");
    assert_eq!(
        error.code, "RATE_LIMIT_EXCEEDED",
        "429 response should have RATE_LIMIT_EXCEEDED code"
    );

    if let Some(details) = error.details.as_object() {
        assert!(
            details.contains_key("retryAfter"),
            "Error details should include retryAfter field"
        );
        let json_retry_after = details
            .get("retryAfter")
            .and_then(|v| v.as_u64())
            .expect("retryAfter should be a number");
        assert_eq!(
            json_retry_after, retry_after_seconds,
            "details.retryAfter should match Retry-After header value"
        );
    } else {
        panic!("Error details should be a JSON object");
    }
}

#[tokio::test]
async fn test_database_error_code_normalization() {
    // Test that all database errors return DATABASE_ERROR code
    use adapteros_server_api::errors::error_to_components;
    use adapteros_core::AosError;

    // Test Sqlx error
    let sqlx_error = AosError::Sqlx("connection failed".to_string());
    let (status, code, _msg) = error_to_components(&sqlx_error);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        code, "DATABASE_ERROR",
        "Sqlx errors should map to DATABASE_ERROR, got: {}",
        code
    );

    // Test Database error
    let db_error = AosError::Database("query failed".to_string());
    let (status, code, _msg) = error_to_components(&db_error);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        code, "DATABASE_ERROR",
        "Database errors should map to DATABASE_ERROR, got: {}",
        code
    );
}

