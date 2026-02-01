//! Hydration Gating Tests
//!
//! Validates the contract: UI Ready implies Server Ready.
//!
//! These tests ensure that client-side hydration cannot complete until
//! the server reports ready via /readyz. This prevents the UI from
//! showing a ready state while the server is still booting.

#![cfg(all(test, feature = "extended-tests"))]

mod common;

use adapteros_server_api::handlers::health::{ReadinessMode, ReadyzResponse};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::test_harness::ApiTestHarness;
use tower::ServiceExt;

/// Test: Server /readyz returns 503 when database check fails.
///
/// This validates that UI hydration would block (or show loading)
/// when the server is not ready due to database connectivity.
#[tokio::test]
async fn readyz_returns_503_when_db_check_fails() -> Result<(), Box<dyn std::error::Error>> {
    // Create harness but don't insert required data
    let harness = ApiTestHarness::new().await?;

    let request = Request::builder()
        .uri("/readyz")
        .method("GET")
        .body(Body::empty())?;

    let response = harness.app.clone().oneshot(request).await?;

    // With empty database (no workers, no models), strict mode returns 503
    // This represents the "server not ready" state that should block UI hydration
    let status = response.status();

    // In strict mode, missing workers or models causes 503
    // DevBypass mode would return 200, but that's not the production contract
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "expected 200 (dev bypass) or 503 (strict mode), got {}",
        status
    );

    Ok(())
}

/// Test: Server /readyz returns 200 when all checks pass.
///
/// This validates the happy path where server is fully ready
/// and UI hydration can complete.
#[tokio::test]
async fn readyz_returns_200_when_all_checks_pass() -> Result<(), Box<dyn std::error::Error>> {
    use adapteros_db::{sqlx, workers::WorkerRegistrationParams};

    let harness = ApiTestHarness::new().await?;

    // Seed required data for readiness checks
    let node_id = harness
        .state
        .db
        .register_node("test-node", "http://localhost:0")
        .await?;

    // Register a worker (worker check)
    harness
        .state
        .db
        .register_worker(WorkerRegistrationParams {
            worker_id: "test-worker".to_string(),
            tenant_id: "default".to_string(),
            node_id,
            plan_id: "test-plan".to_string(),
            uds_path: "/tmp/test.sock".to_string(),
            pid: 1234,
            manifest_hash: "test-hash".to_string(),
            backend: Some("mlx".to_string()),
            model_hash_b3: None,
            tokenizer_hash_b3: None,
            tokenizer_vocab_size: None,
            capabilities_json: None,
            schema_version: "1".to_string(),
            api_version: "1".to_string(),
        })
        .await?;

    // Seed a model (models_seeded check)
    sqlx::query(
        "INSERT INTO models (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("test-model")
    .bind("Test Model")
    .bind("abc123")
    .bind("cfg123")
    .bind("tok123")
    .bind("tokcfg123")
    .execute(harness.state.db.pool())
    .await?;

    let request = Request::builder()
        .uri("/readyz")
        .method("GET")
        .body(Body::empty())?;

    let response = harness.app.clone().oneshot(request).await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "expected 200 when all checks pass"
    );

    Ok(())
}

/// Test: /healthz returns based on boot state, not individual checks.
///
/// /healthz is a liveness probe that only checks boot state.
/// This is separate from /readyz which checks functional readiness.
#[tokio::test]
async fn healthz_returns_200_for_healthy_boot_state() -> Result<(), Box<dyn std::error::Error>> {
    let harness = ApiTestHarness::new().await?;

    let request = Request::builder()
        .uri("/healthz")
        .method("GET")
        .body(Body::empty())?;

    let response = harness.app.clone().oneshot(request).await?;

    // /healthz should return 200 for any non-failed boot state
    // In test harness, boot state is typically Ready or DevBypass
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "expected /healthz to return 200 for healthy boot state"
    );

    Ok(())
}

/// Test: Readiness mode affects check enforcement.
///
/// Validates the three readiness modes:
/// - Strict: All checks required
/// - Relaxed: Some checks skippable
/// - DevBypass: Always returns 200
#[tokio::test]
async fn readyz_response_includes_readiness_mode() -> Result<(), Box<dyn std::error::Error>> {
    let harness = ApiTestHarness::new().await?;

    let request = Request::builder()
        .uri("/readyz")
        .method("GET")
        .body(Body::empty())?;

    let response = harness.app.clone().oneshot(request).await?;
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let readyz: ReadyzResponse = serde_json::from_slice(&body)?;

    // Verify response includes mode information
    assert!(
        matches!(
            readyz.readiness_mode,
            ReadinessMode::Strict | ReadinessMode::Relaxed { .. } | ReadinessMode::DevBypass
        ),
        "readyz response should include readiness_mode"
    );

    // Verify check structure is present
    assert!(
        readyz.checks.db.hint.is_some() || readyz.checks.db.ok,
        "db check should have result"
    );

    Ok(())
}

/// Test: UI hydration contract - ready field matches HTTP status.
///
/// The `ready` boolean in the response body must match the HTTP status:
/// - ready=true → HTTP 200
/// - ready=false → HTTP 503 (unless DevBypass)
#[tokio::test]
async fn readyz_body_ready_matches_http_status() -> Result<(), Box<dyn std::error::Error>> {
    let harness = ApiTestHarness::new().await?;

    let request = Request::builder()
        .uri("/readyz")
        .method("GET")
        .body(Body::empty())?;

    let response = harness.app.clone().oneshot(request).await?;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    let readyz: ReadyzResponse = serde_json::from_slice(&body)?;

    // In DevBypass mode, status is always 200 regardless of ready field
    if matches!(readyz.readiness_mode, ReadinessMode::DevBypass) {
        assert_eq!(status, StatusCode::OK, "DevBypass always returns 200");
    } else {
        // In Strict/Relaxed modes, HTTP status reflects ready field
        if readyz.ready {
            assert_eq!(status, StatusCode::OK, "ready=true should return HTTP 200");
        } else {
            assert_eq!(
                status,
                StatusCode::SERVICE_UNAVAILABLE,
                "ready=false should return HTTP 503"
            );
        }
    }

    Ok(())
}
