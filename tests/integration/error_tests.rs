use axum::{routing::get, Router};
use axum::http::{Request, StatusCode};
use axum::body::Body;
use tower::ServiceExt;
use serde_json::Value;
use crate::adapteros_api::{ApiError, ErrorResponse}; // Assume imports

#[tokio::test]
async fn test_panic_recovery() {
    // Minimal router with panic layer
    let app = Router::new()
        .route("/panic", get(|| async {
            panic!("Mock panic in handler");
        }))
        .layer(crate::middleware::panic_recovery_layer());

    let response = app
        .oneshot(Request::builder().uri("/panic").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let error_resp: ErrorResponse = serde_json::from_value(json).unwrap();
    assert_eq!(error_resp.error, "internal error".to_string());
    assert!(error_resp.details.is_some());
    assert_eq!(error_resp.details.unwrap(), "Internal server error (panic recovered)".to_string());

    // Verify no crash (test passes)
}

#[tokio::test]
async fn test_invalid_json() {
    let app = Router::new()
        .route("/inference", post(|Json(req): Json<InferenceRequest>| async { /* handler */ Ok(Json(InferenceResponse::default())) }))
        .layer(crate::middleware::extractor_error_layer())
        .layer(crate::middleware::error_catcher_layer());

    let invalid_body = r#"{"cpid": "invalid", "max_tokens": "not_number"}"#.as_bytes(); // Invalid types for expected fields

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/inference")
                .header("content-type", "application/json")
                .body(Body::from(invalid_body))
                .unwrap()
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let error_resp: ErrorResponse = serde_json::from_value(json).unwrap();
    assert_eq!(error_resp.error, "bad request".to_string());
    assert_eq!(error_resp.code, "BAD_REQUEST".to_string());
    assert!(error_resp.details.is_some());
    assert!(error_resp.details.unwrap().contains("JSON parse error") || error_resp.details.unwrap().contains("expected"));

    // Verify log emitted (manual check or mock tracing if needed)
}

#[tokio::test]
async fn test_rate_limit_exceeded() {
    let config = RateLimitConfig::default(); // 1 rps, burst 5
    let app = Router::new()
        .route("/inference", post(|_| async { /* mock */ Ok(Json(InferenceResponse::default())) }))
        .layer(ratelimit::rate_limit_layer(config));

    // Burst 5 succeed
    let mut handles = vec![];
    for i in 0..5 {
        let app_clone = app.clone();
        handles.push(tokio::spawn(async move {
            let response = app_clone
                .oneshot(
                    Request::post("/inference")
                        .body(Body::from(serde_json::to_string(&InferenceRequest { cpid: format!("test{}", i), max_tokens: 100 }).unwrap()))
                        .unwrap()
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }

    // 6 more fail with 429
    let mut fail_handles = vec![];
    for i in 0..6 {
        let app_clone = app.clone();
        fail_handles.push(tokio::spawn(async move {
            let response = app_clone
                .oneshot(
                    Request::post("/inference")
                        .body(Body::from(serde_json::to_string(&InferenceRequest { cpid: format!("fail{}", i), max_tokens: 100 }).unwrap()))
                        .unwrap()
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            let headers = response.headers();
            assert!(headers.get("Retry-After").is_some());
            let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
            let json: Value = serde_json::from_slice(&body).unwrap();
            let error_resp: ErrorResponse = serde_json::from_value(json).unwrap();
            assert_eq!(error_resp.code, "RATE_LIMIT_EXCEEDED".to_string());
        }));
    }
    for handle in fail_handles {
        handle.await.unwrap();
    }
}
