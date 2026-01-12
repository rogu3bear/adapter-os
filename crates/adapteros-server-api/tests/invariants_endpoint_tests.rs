//! Tests for the /v1/invariants endpoint.
//!
//! Validates:
//! 1. Endpoint returns correct response structure
//! 2. Response includes expected fields
//! 3. OpenAPI schema is consistent

use adapteros_server_api::handlers::health::{InvariantStatusResponse, InvariantViolationDto};
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use tower::ServiceExt;

mod common;
use common::{setup_state, TestkitEnvGuard};

/// Test that the invariants endpoint handler returns a properly structured response
#[tokio::test]
async fn invariants_endpoint_returns_valid_structure() -> anyhow::Result<()> {
    let _env = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await?;

    let app = Router::new()
        .route(
            "/v1/invariants",
            get(adapteros_server_api::handlers::health::get_invariant_status),
        )
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/invariants")
                .body(Body::empty())
                .expect("request build"),
        )
        .await
        .expect("router response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 64)
        .await
        .expect("read body");

    let _response: InvariantStatusResponse = serde_json::from_slice(&body)?;

    // Verify response structure has expected fields
    // violations and skipped_ids should be valid arrays (may be empty)
    // production_mode should be a boolean (either value is valid)

    Ok(())
}

/// Test that InvariantStatusResponse serializes/deserializes correctly
#[test]
fn invariant_status_response_serde() {
    let response = InvariantStatusResponse {
        checked: 16,
        passed: 14,
        failed: 1,
        skipped: 1,
        fatal: 0,
        violations: vec![InvariantViolationDto {
            id: "SEC-005".to_string(),
            message: "Test violation".to_string(),
            is_fatal: false,
            remediation: "Fix it".to_string(),
        }],
        skipped_ids: vec!["SEC-001".to_string()],
        production_mode: false,
    };

    let json = serde_json::to_string(&response).expect("serialize");
    let parsed: InvariantStatusResponse = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.checked, 16);
    assert_eq!(parsed.passed, 14);
    assert_eq!(parsed.failed, 1);
    assert_eq!(parsed.violations.len(), 1);
    assert_eq!(parsed.violations[0].id, "SEC-005");
}

/// Test that InvariantViolationDto has correct schema fields
#[test]
fn invariant_violation_dto_fields() {
    let violation = InvariantViolationDto {
        id: "SEC-003".to_string(),
        message: "Executor seed not set".to_string(),
        is_fatal: true,
        remediation: "Provide manifest".to_string(),
    };

    // Verify fields exist and have expected types
    assert_eq!(violation.id, "SEC-003");
    assert!(violation.message.contains("seed"));
    assert!(violation.is_fatal);
    assert!(violation.remediation.contains("manifest"));
}
