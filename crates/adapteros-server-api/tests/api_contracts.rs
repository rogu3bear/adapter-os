//! API Contract Tests
//! Verifies API contracts between frontend and backend

use adapteros_server_api::create_app;
use axum::{
    body::to_bytes,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

mod common;

#[tokio::test]
async fn test_adapter_list_response_contract() {
    // Enable dev no-auth to exercise the real router without JWT setup.
    std::env::set_var("AOS_DEV_NO_AUTH", "1");

    let state = common::setup_state(None)
        .await
        .expect("failed to set up state");
    let app = create_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/adapters")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);

    // Correlate with backend logs via request ID header
    let rid = response.headers().get("x-request-id");
    assert!(rid.is_some(), "response should include x-request-id");

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let json: Value = serde_json::from_slice(&body_bytes).expect("valid JSON array");
    assert!(
        json.is_array(),
        "GET /v1/adapters should return an array even when empty"
    );

    if let Some(first) = json.as_array().and_then(|arr| arr.first()) {
        assert!(
            first.get("id").is_some(),
            "adapter objects should include an id field"
        );
    }
}

#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_adapter_detail_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_training_job_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_tenant_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_policy_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_metrics_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_audit_log_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_stack_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_worker_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_node_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_health_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_error_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_pagination_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_filter_request_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_sort_request_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_inference_request_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_inference_response_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_streaming_event_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_telemetry_event_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_authentication_contract() {}
#[test]
#[ignore = "Pending API refactoring [tracking: STAB-IGN-001]"]
fn test_authorization_contract() {}
