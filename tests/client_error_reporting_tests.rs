//! Integration tests for client error reporting endpoints
//!
//! Tests the `/v1/telemetry/client-errors` and `/v1/telemetry/client-errors/anonymous`
//! endpoints that receive error reports from the UI for persistent logging.
//!
//! Run with: `cargo test --test client_error_reporting_tests`

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::test_harness::ApiTestHarness;
use serde_json::json;
use tower::ServiceExt;

/// Helper to make a POST request with JSON body
async fn post_json(
    app: &axum::Router,
    path: &str,
    body: serde_json::Value,
    auth_token: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");

    if let Some(token) = auth_token {
        builder = builder.header("authorization", format!("Bearer {}", token));
    }

    let request = builder
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        json!({ "raw": String::from_utf8_lossy(&bytes).to_string() })
    });

    (status, json)
}

/// Valid client error report payload
fn valid_error_report() -> serde_json::Value {
    json!({
        "error_type": "Network",
        "message": "Connection refused to /v1/health",
        "code": "NETWORK_ERROR",
        "failure_code": null,
        "http_status": null,
        "page": "/dashboard",
        "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        "timestamp": "2026-01-08T20:00:00Z",
        "details": null
    })
}

// ============================================================================
// Anonymous Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_anonymous_endpoint_success() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        valid_error_report(),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "Expected 201 Created");
    assert!(
        response.get("error_id").is_some(),
        "Response should contain error_id"
    );
    assert!(
        response.get("received_at").is_some(),
        "Response should contain received_at"
    );

    // Verify error_id is a valid UUID format
    let error_id = response["error_id"].as_str().unwrap();
    assert!(
        uuid::Uuid::parse_str(error_id).is_ok(),
        "error_id should be valid UUID"
    );
}

#[tokio::test]
async fn test_anonymous_endpoint_empty_message_rejected() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let payload = json!({
        "error_type": "Network",
        "message": "   ",
        "user_agent": "test",
        "timestamp": "2026-01-08T20:00:00Z"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Expected 400 Bad Request, got {}: {:?}", status, response);
    // Server returns error details in either "error" or "message" field depending on middleware
    let error_text = response["error"]
        .as_str()
        .or_else(|| response["message"].as_str())
        .unwrap_or("");
    assert!(
        error_text.to_lowercase().contains("message") || response["code"].as_str() == Some("BAD_REQUEST"),
        "Error should indicate bad request, got: {:?}",
        response
    );
}

#[tokio::test]
async fn test_anonymous_endpoint_invalid_timestamp_rejected() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let payload = json!({
        "error_type": "Network",
        "message": "Test error",
        "user_agent": "test",
        "timestamp": "not-a-valid-timestamp"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Expected 400 Bad Request, got {}: {:?}", status, response);
    assert_eq!(
        response["code"].as_str(),
        Some("BAD_REQUEST"),
        "Should return BAD_REQUEST code"
    );
}

#[tokio::test]
async fn test_anonymous_endpoint_message_too_long_rejected() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    // Message over 2000 chars
    let long_message = "x".repeat(2001);
    let payload = json!({
        "error_type": "Network",
        "message": long_message,
        "user_agent": "test",
        "timestamp": "2026-01-08T20:00:00Z"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Expected 400 Bad Request, got {}: {:?}", status, response);
    assert_eq!(
        response["code"].as_str(),
        Some("BAD_REQUEST"),
        "Should return BAD_REQUEST code"
    );
}

#[tokio::test]
async fn test_anonymous_endpoint_empty_error_type_rejected() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let payload = json!({
        "error_type": "  ",
        "message": "Test error",
        "user_agent": "test",
        "timestamp": "2026-01-08T20:00:00Z"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Expected 400 Bad Request, got {}: {:?}", status, response);
    assert_eq!(
        response["code"].as_str(),
        Some("BAD_REQUEST"),
        "Should return BAD_REQUEST code"
    );
}

// ============================================================================
// Authenticated Endpoint Tests
// ============================================================================
// Note: Authentication is disabled in test harness by default.
// These tests are ignored unless run with full auth setup.

#[tokio::test]
#[ignore = "Auth disabled in test harness - run with full server setup"]
async fn test_authenticated_endpoint_success() {
    let mut harness = ApiTestHarness::new().await.expect("Failed to init harness");
    let token = harness.authenticate().await.expect("Failed to authenticate");

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors",
        valid_error_report(),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "Expected 201 Created");
    assert!(
        response.get("error_id").is_some(),
        "Response should contain error_id"
    );
    assert!(
        response.get("received_at").is_some(),
        "Response should contain received_at"
    );
}

#[tokio::test]
async fn test_authenticated_endpoint_requires_auth() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let (status, _response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors",
        valid_error_report(),
        None,
    )
    .await;

    // Without auth, should get 401 Unauthorized
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::NOT_IMPLEMENTED,
        "Expected 401 Unauthorized or 501 Not Implemented, got {}",
        status
    );
}

#[tokio::test]
#[ignore = "Auth disabled in test harness - run with full server setup"]
async fn test_authenticated_endpoint_empty_message_rejected() {
    let mut harness = ApiTestHarness::new().await.expect("Failed to init harness");
    let token = harness.authenticate().await.expect("Failed to authenticate");

    let payload = json!({
        "error_type": "Network",
        "message": "",
        "user_agent": "test",
        "timestamp": "2026-01-08T20:00:00Z"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors",
        payload,
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "Expected 400 Bad Request");
    assert_eq!(
        response["code"].as_str(),
        Some("BAD_REQUEST"),
        "Should return BAD_REQUEST code"
    );
}

// ============================================================================
// Response Structure Tests
// ============================================================================

#[tokio::test]
async fn test_response_structure_conforms_to_spec() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        valid_error_report(),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);

    // Verify error_id is UUID
    let error_id = response["error_id"].as_str().expect("error_id should be string");
    uuid::Uuid::parse_str(error_id).expect("error_id should be valid UUID");

    // Verify received_at is ISO 8601 (RFC 3339)
    let received_at = response["received_at"]
        .as_str()
        .expect("received_at should be string");
    chrono::DateTime::parse_from_rfc3339(received_at)
        .expect("received_at should be valid RFC 3339 timestamp");
}

#[tokio::test]
async fn test_all_optional_fields_accepted() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    // Full payload with all optional fields
    let payload = json!({
        "error_type": "Http",
        "message": "Server returned 500",
        "code": "SERVER_ERROR",
        "failure_code": "InternalError",
        "http_status": 500,
        "page": "/api/v1/adapters",
        "user_agent": "Mozilla/5.0",
        "timestamp": "2026-01-08T20:30:00Z",
        "details": {
            "request_id": "abc-123",
            "retry_count": 3
        }
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "Should accept all optional fields");
    assert!(response.get("error_id").is_some());
}

#[tokio::test]
async fn test_minimal_valid_payload_accepted() {
    let harness = ApiTestHarness::new().await.expect("Failed to init harness");

    // Minimal payload with only required fields
    let payload = json!({
        "error_type": "Network",
        "message": "Timeout",
        "user_agent": "test",
        "timestamp": "2026-01-08T20:00:00Z"
    });

    let (status, response) = post_json(
        &harness.app,
        "/v1/telemetry/client-errors/anonymous",
        payload,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "Should accept minimal payload");
    assert!(response.get("error_id").is_some());
}
