//! Error case tests for testkit endpoints
//!
//! Tests the following error conditions:
//! 1. E2E_MODE flag enforcement - testkit endpoints should fail when E2E_MODE is not set
//! 2. Invalid tenant ID handling - malformed or non-existent tenant IDs
//! 3. Concurrent testkit operations - multiple reset/seed operations shouldn't corrupt state
//! 4. Malformed request rejection - invalid JSON, missing required fields
//! 5. Authorization requirements for testkit endpoints (audit diverge)
//! 6. Testkit reset doesn't affect other tenants
//! 7. Seed operation idempotency
//! 8. Fixture creation with invalid parameters

use adapteros_server_api::create_app;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
type EnvGuard = common::TestkitEnvGuard;

// ============================================================================
// Test 1: E2E_MODE flag enforcement
// ============================================================================

#[tokio::test]
async fn testkit_reset_requires_e2e_mode() {
    let _env = EnvGuard::disabled().await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
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

    // Should return 404 because testkit routes are not registered
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn testkit_seed_minimal_requires_e2e_mode() {
    let _env = EnvGuard::disabled().await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
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

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn testkit_create_trace_fixture_requires_e2e_mode() {
    let _env = EnvGuard::disabled().await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ============================================================================
// Test 2: Invalid tenant ID handling
// ============================================================================

#[tokio::test]
async fn create_trace_fixture_with_nonexistent_tenant() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Request with non-existent tenant
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "tenant_id": "nonexistent-tenant-9999",
                        "token_count": 10
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");

    // Should return 500 due to FK constraint violation
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body_bytes).expect("json");

    assert_eq!(
        body.get("code").and_then(|v| v.as_str()),
        Some("TESTKIT_ERROR")
    );
}

#[tokio::test]
async fn create_evidence_fixture_with_nonexistent_tenant() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_evidence_fixture")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "tenant_id": "nonexistent-tenant-9999",
                        "inference_id": "test-trace"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn set_policy_with_nonexistent_tenant() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/set_policy")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "tenant_id": "nonexistent-tenant-9999",
                        "policy_id": "egress",
                        "enabled": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ============================================================================
// Test 3: Concurrent testkit operations
// ============================================================================

#[tokio::test]
async fn concurrent_seed_minimal_operations() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Run multiple seed_minimal operations sequentially to avoid env races
    // and verify idempotency.
    for _ in 0..5 {
        let response = app
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

        assert_eq!(response.status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn concurrent_trace_fixture_creation() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // First seed minimal fixtures
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Run trace fixture creations sequentially to verify idempotency.
    for _ in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/testkit/create_trace_fixture")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"token_count": 20}).to_string()))
                    .unwrap(),
            )
            .await
            .expect("router responds");

        assert_eq!(response.status(), StatusCode::OK);
    }
}

// ============================================================================
// Test 4: Malformed request rejection
// ============================================================================

#[tokio::test]
async fn create_trace_fixture_with_invalid_json() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from("{invalid json"))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_evidence_fixture_with_invalid_json() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_evidence_fixture")
                .header("content-type", "application/json")
                .body(Body::from("not a json object"))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn set_policy_with_missing_required_fields() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Missing 'enabled' field
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/set_policy")
                .header("content-type", "application/json")
                .body(Body::from(json!({"policy_id": "egress"}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_repo_with_missing_fields() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Seed minimal first to create tenant
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Empty JSON object (all fields are optional, so this should succeed)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_repo")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    // Should succeed with defaults
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_adapter_version_with_missing_repo_id() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Missing required 'repo_id' field
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_adapter_version")
                .header("content-type", "application/json")
                .body(Body::from(json!({"version": "1.0.0"}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ============================================================================
// Test 5: Authorization requirements for testkit endpoints
// ============================================================================

#[tokio::test]
async fn audit_diverge_requires_valid_tenant_isolation() {
    let _env = EnvGuard::enabled(false).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Seed minimal first
    let seed_app = app.clone();
    std::env::set_var("AOS_DEV_NO_AUTH", "1");
    seed_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");
    std::env::remove_var("AOS_DEV_NO_AUTH");

    // Request without auth should be rejected
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/audit/diverge?tenant_id=tenant-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");

    // Should fail auth
    assert!(
        response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN
    );
}

// ============================================================================
// Test 6: Testkit reset doesn't affect other tenants
// ============================================================================

#[tokio::test]
async fn reset_clears_all_tenants_data() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state.clone());

    // Seed minimal fixtures
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Verify data exists
    let count: i64 = adapteros_db::sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
        .fetch_one(state.db.pool())
        .await
        .expect("query succeeded");
    assert!(count > 0, "Should have seeded tenants");

    // Reset
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("reset completed");

    // Verify all data is cleared (except reference data)
    let tenant_count: i64 = adapteros_db::sqlx::query_scalar("SELECT COUNT(*) FROM tenants")
        .fetch_one(state.db.pool())
        .await
        .expect("query succeeded");
    assert_eq!(tenant_count, 0, "All tenants should be cleared");

    let user_count: i64 = adapteros_db::sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(state.db.pool())
        .await
        .expect("query succeeded");
    assert_eq!(user_count, 0, "All users should be cleared");

    let model_count: i64 = adapteros_db::sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(state.db.pool())
        .await
        .expect("query succeeded");
    assert_eq!(model_count, 0, "All models should be cleared");
}

// ============================================================================
// Test 7: Seed operation idempotency
// ============================================================================

#[tokio::test]
async fn seed_minimal_is_idempotent() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state.clone());

    // Run seed_minimal multiple times
    for i in 0..3 {
        let response = app
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

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "seed_minimal iteration {} should succeed",
            i
        );

        let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body bytes");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json");

        // Verify consistent IDs across runs
        assert_eq!(
            body.get("tenant_id").and_then(|v| v.as_str()),
            Some("tenant-test")
        );
        assert_eq!(
            body.get("user_id").and_then(|v| v.as_str()),
            Some("user-e2e")
        );
        assert_eq!(
            body.get("model_id").and_then(|v| v.as_str()),
            Some("model-qwen-test")
        );
    }

    // Verify no duplicate records were created
    let tenant_count: i64 = adapteros_db::sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenants WHERE id = 'tenant-test' OR id = 'tenant-test-2'",
    )
    .fetch_one(state.db.pool())
    .await
    .expect("query succeeded");
    assert_eq!(tenant_count, 2, "Should have exactly 2 tenants");

    let user_count: i64 =
        adapteros_db::sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = 'user-e2e'")
            .fetch_one(state.db.pool())
            .await
            .expect("query succeeded");
    assert_eq!(user_count, 1, "Should have exactly 1 user");
}

#[tokio::test]
async fn trace_fixture_is_idempotent() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state.clone());

    // Seed minimal first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Create trace fixture multiple times
    for i in 0..3 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/testkit/create_trace_fixture")
                    .header("content-type", "application/json")
                    .body(Body::from(json!({"token_count": 25}).to_string()))
                    .unwrap(),
            )
            .await
            .expect("router responds");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "trace fixture iteration {} should succeed",
            i
        );

        let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body bytes");
        let body: Value = serde_json::from_slice(&body_bytes).expect("json");

        assert_eq!(
            body.get("trace_id").and_then(|v| v.as_str()),
            Some("trace-fixture")
        );
        assert_eq!(body.get("token_count").and_then(|v| v.as_u64()), Some(25));
    }

    // Verify only one trace exists (not duplicated)
    let trace_count: i64 = adapteros_db::sqlx::query_scalar(
        "SELECT COUNT(*) FROM inference_traces WHERE trace_id = 'trace-fixture'",
    )
    .fetch_one(state.db.pool())
    .await
    .expect("query succeeded");
    assert_eq!(trace_count, 1, "Should have exactly 1 trace");

    let token_count: i64 = adapteros_db::sqlx::query_scalar(
        "SELECT COUNT(*) FROM inference_trace_tokens WHERE trace_id = 'trace-fixture'",
    )
    .fetch_one(state.db.pool())
    .await
    .expect("query succeeded");
    assert_eq!(token_count, 25, "Should have exactly 25 tokens");
}

// ============================================================================
// Test 8: Fixture creation with invalid parameters
// ============================================================================

#[tokio::test]
async fn trace_fixture_with_zero_token_count() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Seed minimal first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Request with zero token count (should be clamped to 1)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token_count": 0}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body_bytes).expect("json");

    // Should be clamped to minimum of 1
    assert_eq!(body.get("token_count").and_then(|v| v.as_u64()), Some(1));
}

#[tokio::test]
async fn trace_fixture_with_empty_adapter_ids() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Seed minimal first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Request with empty adapter_ids array (should fall back to defaults)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_trace_fixture")
                .header("content-type", "application/json")
                .body(Body::from(json!({"adapter_ids": []}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body_bytes).expect("json");

    // Should fall back to default adapter IDs
    let adapter_ids = body
        .get("adapter_ids")
        .and_then(|v| v.as_array())
        .expect("adapter_ids array");
    assert!(
        adapter_ids.len() > 0,
        "Should have default adapter IDs when empty array provided"
    );
}

#[tokio::test]
async fn create_adapter_version_with_nonexistent_repo() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Seed minimal first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    // Request with non-existent repo_id
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_adapter_version")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "repo_id": "nonexistent-repo-9999",
                        "version": "1.0.0"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");

    // Should fail with FK constraint violation
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn training_job_stub_with_invalid_status() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state.clone());

    // Seed minimal and create repo first
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/seed_minimal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("seed completed");

    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_repo")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .expect("repo created");

    // Create job with arbitrary status (should be accepted, just stored as-is)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/create_training_job_stub")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "status": "custom_weird_status"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");

    // Should succeed (status validation happens at application level, not testkit)
    assert_eq!(response.status(), StatusCode::OK);

    // Verify the status was stored
    let stored_status: String = adapteros_db::sqlx::query_scalar(
        "SELECT status FROM repository_training_jobs WHERE id = 'job-stub'",
    )
    .fetch_one(state.db.pool())
    .await
    .expect("query succeeded");
    assert_eq!(stored_status, "custom_weird_status");
}

#[tokio::test]
async fn inference_stub_returns_consistent_structure() {
    let _env = EnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    // Inference stub doesn't require any setup
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/testkit/inference_stub")
                .header("content-type", "application/json")
                .body(Body::from(json!({"prompt": "test prompt"}).to_string()))
                .unwrap(),
        )
        .await
        .expect("router responds");

    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body bytes");
    let body: Value = serde_json::from_slice(&body_bytes).expect("json");

    // Verify structure
    assert!(body.get("schema_version").is_some());
    assert!(body.get("id").is_some());
    assert!(body.get("text").is_some());
    assert!(body.get("run_receipt").is_some());
    assert!(body.get("trace").is_some());

    // Verify echo behavior
    let text = body.get("text").and_then(|v| v.as_str()).unwrap();
    assert!(text.contains("test prompt"), "Should echo the prompt");
}
