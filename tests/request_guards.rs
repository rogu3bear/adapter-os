//! Guardrail coverage for inference requests (empty prompt and oversized context).

use adapteros_api_types::ErrorResponse;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::test_harness::ApiTestHarness;

fn parse_error(bytes: &[u8]) -> (String, String) {
    if let Ok(err) = serde_json::from_slice::<ErrorResponse>(bytes) {
        return (err.code, err.message);
    }
    // Fallback/Legacy check removed as we consolidated error types
    panic!("parse error response: {}", String::from_utf8_lossy(bytes));
}

#[tokio::test]
async fn empty_prompt_returns_bad_request() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("failed to init API harness");
    let token = harness.authenticate().await.expect("auth");
    let app = harness.app.clone();

    let request = Request::builder()
        .method("POST")
        .uri("/v1/infer")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "identity")
        .body(Body::from(
            json!({"prompt": "", "max_tokens": 4}).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let (code, message) = parse_error(&bytes);
    assert_eq!(code, "BAD_REQUEST");
    assert!(
        message.to_lowercase().contains("prompt"),
        "expected prompt error, got {:?}",
        message
    );
}

#[tokio::test]
async fn huge_context_returns_clear_error() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("failed to init API harness");
    let token = harness.authenticate().await.expect("auth");
    let app = harness.app.clone();

    let oversized_prompt = "context ".repeat(50_000);
    let request = Request::builder()
        .method("POST")
        .uri("/v1/infer")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .header("Accept-Encoding", "identity")
        .body(Body::from(
            json!({"prompt": oversized_prompt, "max_tokens": 4}).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let (code, message) = parse_error(&bytes);
    assert_eq!(code, "BAD_REQUEST");
    assert!(
        message.to_lowercase().contains("context")
            || message.to_lowercase().contains("prompt too long"),
        "expected context window error, got {:?}",
        message
    );
}
