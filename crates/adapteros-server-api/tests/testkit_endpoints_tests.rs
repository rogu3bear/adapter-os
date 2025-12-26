use adapteros_server_api::create_app;
use axum::{
    body::to_bytes,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;

#[tokio::test]
async fn testkit_routes_toggle_with_env() {
    let _env = common::TestkitEnvGuard::disabled().await;

    let state_disabled = common::setup_state(None).await.expect("setup state");
    let app_disabled = create_app(state_disabled);

    let response = app_disabled
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Happy path with E2E mode enabled
    common::set_testkit_env(true);

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Reset (truncate) database
    let reset_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    let reset_status = reset_resp.status();
    let reset_body = to_bytes(reset_resp.into_body(), 1024 * 1024)
        .await
        .expect("body");
    if reset_status != StatusCode::OK {
        panic!(
            "reset failed with status {} and body: {}",
            reset_status,
            String::from_utf8_lossy(&reset_body)
        );
    }

    // Seed minimal fixtures
    let seed_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    let seed_status = seed_resp.status();
    let seed_body = to_bytes(seed_resp.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    if seed_status != StatusCode::OK {
        panic!(
            "seed_minimal failed with status {} and body: {}",
            seed_status,
            String::from_utf8_lossy(&seed_body)
        );
    }
    let seed_json: Value = serde_json::from_slice(&seed_body).expect("seed json");
    assert_eq!(
        seed_json
            .get("secondary_tenant_id")
            .and_then(|v| v.as_str()),
        Some("tenant-test-2")
    );

    // Trace fixture with deterministic IDs
    let trace_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "token_count": 50 }).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(trace_resp.status(), StatusCode::OK);
    let trace_body = to_bytes(trace_resp.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let trace_json: Value = serde_json::from_slice(&trace_body).expect("trace json");
    assert_eq!(
        trace_json.get("trace_id").and_then(|v| v.as_str()),
        Some("trace-fixture")
    );
    assert_eq!(
        trace_json.get("token_count").and_then(|v| v.as_u64()),
        Some(50)
    );

    // Evidence fixture linked to trace
    let evidence_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_evidence_fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "inference_id": "trace-fixture" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(evidence_resp.status(), StatusCode::OK);
    let evidence_body = to_bytes(evidence_resp.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let evidence_json: Value = serde_json::from_slice(&evidence_body).expect("evidence json");
    assert_eq!(
        evidence_json.get("evidence_id").and_then(|v| v.as_str()),
        Some("evidence-fixture")
    );
    assert_eq!(
        evidence_json.get("inference_id").and_then(|v| v.as_str()),
        Some("trace-fixture")
    );

    // Toggle policy via fixture endpoint
    let policy_resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/set_policy_fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "tenant_id": "tenant-test",
                        "policy_id": "egress",
                        "enabled": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(policy_resp.status(), StatusCode::OK);
}
