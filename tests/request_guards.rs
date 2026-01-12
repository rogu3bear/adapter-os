//! Guardrail coverage for inference requests (empty prompt and oversized context).

use adapteros_server_api::types::ApiErrorBody;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

mod common;
use common::test_harness::ApiTestHarness;

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
        .body(Body::from(
            json!({"prompt": "", "max_tokens": 4}).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let err: ApiErrorBody = serde_json::from_slice(&bytes).expect("parse error response");
    assert_eq!(err.code, "BAD_REQUEST");
    assert!(
        err.message.to_lowercase().contains("prompt"),
        "expected prompt error, got {:?}",
        err.message
    );
}

#[tokio::test]
async fn huge_context_returns_clear_error() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("failed to init API harness");
    let token = harness.authenticate().await.expect("auth");
    let app = harness.app.clone();

    let max_post_bytes: usize = 10 * 1024 * 1024;
    let chunk = "context ";
    let repeats = max_post_bytes / chunk.len() + 1;
    let oversized_prompt = chunk.repeat(repeats);
    let body = json!({"prompt": oversized_prompt, "max_tokens": 4}).to_string();
    let request = Request::builder()
        .method("POST")
        .uri("/v1/infer")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Length", body.len().to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let response = app.oneshot(request).await.expect("execute request");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let err: ApiErrorBody = serde_json::from_slice(&bytes).expect("parse error response");
    assert!(
        err.message.to_lowercase().contains("payload")
            || err.message.to_lowercase().contains("too large"),
        "expected payload size error, got {:?}",
        err.message
    );
}
