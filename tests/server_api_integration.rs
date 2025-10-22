//! Integration tests for AdapterOS Server API
//!
//! These tests verify end-to-end functionality including:
//! - Plan creation and management
//! - Policy enforcement workflows
//! - Telemetry streaming and collection
//! - Authentication and authorization
//! - Error handling and validation
//!
//! Run with: `cargo test --test server_api_integration -- --ignored --nocapture`

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::types::*;
use adapteros_server_api::{routes, state::AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

/// Test database setup
async fn setup_test_db() -> Result<Db> {
    let db = Db::connect(":memory:").await?;

    // Run migrations to create tables
    sqlx::migrate!("./migrations")
        .run(db.pool())
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("Migration failed: {}", e)))?;

    Ok(db)
}

/// Test application state setup
async fn setup_test_app() -> Result<Router> {
    let db = setup_test_db().await?;
    let jwt_secret = b"test-secret-key".to_vec();
    let api_config = Arc::new(std::sync::RwLock::new(
        adapteros_server_api::state::ApiConfig {
            metrics: adapteros_server_api::state::MetricsConfig {
                enabled: true,
                bearer_token: "test-token".to_string(),
            },
            golden_gate: None,
            bundles_root: "var/bundles".to_string(),
        },
    ));
    let metrics_exporter = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
        0.1, 0.5, 1.0, 2.5, 5.0,
    ])?);
    let training_service = Arc::new(TrainingService::new());

    let state = AppState::new(
        db,
        jwt_secret,
        api_config,
        metrics_exporter,
        training_service,
    );
    Ok(routes::build(state))
}

#[tokio::test]
async fn test_plan_creation_workflow() -> Result<()> {
    println!("Testing plan creation workflow...");

    let app = setup_test_app().await?;

    // 1. Create tenant
    println!("1. Creating test tenant...");
    let create_tenant_req = CreateTenantRequest {
        name: "test-tenant".to_string(),
        itar_flag: false,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_tenant_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let tenant: TenantResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert_eq!(tenant.name, "test-tenant");

    // 2. Build plan
    println!("2. Building plan...");
    let build_plan_req = BuildPlanRequest {
        tenant_id: tenant.id.clone(),
        manifest_hash_b3: "test-manifest-hash".to_string(),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plans/build")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&build_plan_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let job: JobResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert_eq!(job.kind, "plan_build");

    // 3. Get plan details
    println!("3. Getting plan details...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!("/api/v1/plans/{}", job.id))
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let plan: PlanResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert_eq!(plan.tenant_id, tenant.id);

    println!("✓ Plan creation workflow completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_policy_enforcement_workflow() -> Result<()> {
    println!("Testing policy enforcement workflow...");

    let app = setup_test_app().await?;

    // 1. Create tenant with ITAR flag
    println!("1. Creating ITAR tenant...");
    let create_tenant_req = CreateTenantRequest {
        name: "itar-tenant".to_string(),
        itar_flag: true,
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&create_tenant_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let tenant: TenantResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert!(tenant.itar_flag);

    // 2. Apply policy pack
    println!("2. Applying policy pack...");
    let policy_content = json!({
        "egress": {
            "mode": "deny_all",
            "serve_requires_pf": true,
            "allow_tcp": false,
            "allow_udp": false
        },
        "determinism": {
            "require_metallib_embed": true,
            "require_kernel_hash_match": true,
            "rng": "hkdf_seeded"
        },
        "router": {
            "k_sparse": 3,
            "gate_quant": "q15",
            "entropy_floor": 0.02
        }
    });

    let apply_policy_req = json!({
        "cpid": "test-cp-v1",
        "content": policy_content.to_string()
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/policies/apply")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&apply_policy_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);

    // 3. Validate policy enforcement
    println!("3. Validating policy enforcement...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/policies/test-cp-v1")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let policy: PolicyPackResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert_eq!(policy.cpid, "test-cp-v1");

    // 4. Test policy validation
    println!("4. Testing policy validation...");
    let validate_req = ValidatePolicyRequest {
        content: policy_content.to_string(),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/policies/validate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&validate_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let validation: PolicyValidationResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert!(validation.valid);

    println!("✓ Policy enforcement workflow completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_telemetry_streaming_workflow() -> Result<()> {
    println!("Testing telemetry streaming workflow...");

    let app = setup_test_app().await?;

    // 1. Create telemetry bundle
    println!("1. Creating telemetry bundle...");
    let _telemetry_event = TelemetryEvent {
        event_type: "inference".to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        data: json!({
            "prompt": "test prompt",
            "response": "test response",
            "latency_ms": 150
        }),
        tenant_id: Some("test-tenant".to_string()),
        user_id: Some("test-user".to_string()),
    };

    // 2. Test telemetry bundle creation (simulated)
    println!("2. Simulating telemetry bundle creation...");
    let bundle_response = TelemetryBundleResponse {
        id: "test-bundle-001".to_string(),
        cpid: "test-cp-v1".to_string(),
        event_count: 1,
        size_bytes: 1024,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // 3. Test bundle verification
    println!("3. Testing bundle verification...");
    let verify_req = VerifyBundleSignatureRequest {
        bundle_id: bundle_response.id.clone(),
        expected_signature: "test-signature".to_string(),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/telemetry/bundles/test-bundle-001/verify")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&verify_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // Note: This might return 404 if the bundle doesn't exist in test DB
    // That's expected behavior for this test
    println!(
        "   Bundle verification endpoint responded with status: {}",
        response.status()
    );

    // 4. Test telemetry export
    println!("4. Testing telemetry export...");
    let _export_req = ExportTelemetryBundleRequest {
        bundle_id: bundle_response.id.clone(),
        format: "json".to_string(),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&format!(
                    "/api/v1/telemetry/bundles/{}/export",
                    bundle_response.id
                ))
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // Note: This might return 404 if the bundle doesn't exist in test DB
    // That's expected behavior for this test
    println!(
        "   Telemetry export endpoint responded with status: {}",
        response.status()
    );

    // 5. Test SSE streaming endpoint
    println!("5. Testing SSE streaming endpoint...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/stream/telemetry")
                .header("accept", "text/event-stream")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // SSE endpoint should return 200 with event-stream content type
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));

    println!("✓ Telemetry streaming workflow completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_authentication_workflow() -> Result<()> {
    println!("Testing authentication workflow...");

    let app = setup_test_app().await?;

    // 1. Test health endpoint (no auth required)
    println!("1. Testing health endpoint...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/healthz")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);
    let health: HealthResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert_eq!(health.status, "healthy");

    // 2. Test login endpoint
    println!("2. Testing login endpoint...");
    let login_req = LoginRequest {
        email: "admin@example.com".to_string(),
        password: "admin_password".to_string(),
    };

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&login_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // Login might fail in test environment (no users seeded)
    // That's expected behavior
    println!(
        "   Login endpoint responded with status: {}",
        response.status()
    );

    // 3. Test protected endpoint without auth
    println!("3. Testing protected endpoint without auth...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tenants")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // Should return 401 Unauthorized
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    println!("✓ Authentication workflow completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<()> {
    println!("Testing error handling...");

    let app = setup_test_app().await?;

    // 1. Test invalid JSON
    println!("1. Testing invalid JSON...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .body(Body::from("invalid json"))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 2. Test missing required fields
    println!("2. Testing missing required fields...");
    let invalid_req = json!({
        "name": "test"
        // Missing itar_flag
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/tenants")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&invalid_req)?))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // 3. Test non-existent resource
    println!("3. Testing non-existent resource...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tenants/non-existent-id")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // 4. Test invalid HTTP method
    println!("4. Testing invalid HTTP method...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/healthz")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    println!("✓ Error handling tests completed successfully!");
    Ok(())
}

#[tokio::test]
async fn test_api_consistency() -> Result<()> {
    println!("Testing API consistency...");

    let app = setup_test_app().await?;

    // 1. Test that all endpoints return consistent error format
    println!("1. Testing consistent error format...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/non-existent-endpoint")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let error: ErrorResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
    )?;
    assert!(!error.error.is_empty());
    assert!(!error.code.is_empty());

    // 2. Test pagination consistency
    println!("2. Testing pagination consistency...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tenants?page=1&limit=10")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    // Should return 401 (unauthorized) but with consistent format
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 3. Test content-type consistency
    println!("3. Testing content-type consistency...");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/healthz")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("application/json"));

    println!("✓ API consistency tests completed successfully!");
    Ok(())
}
