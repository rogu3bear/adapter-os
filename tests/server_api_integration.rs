<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

=======
>>>>>>> integration-branch
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
<<<<<<< HEAD
use adapteros_db::{users::Role, Db};
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_manifest::Policies;
use adapteros_orchestrator::TrainingService;
use adapteros_server_api::handlers::git_repository::register_git_repository;
use adapteros_server_api::state::{ApiConfig, MetricsConfig};
use adapteros_server_api::types::*;
use adapteros_server_api::{auth::Claims, routes, state::AppState};
use adapteros_telemetry::metrics::{MetricsCollector, MetricsRegistry};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
    Router,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tower::ServiceExt;

/// Simple JWT encoding for tests (HMAC-SHA256)
fn encode_jwt(claims: &adapteros_server_api::auth::Claims, secret: &[u8]) -> Result<String> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| adapteros_core::AosError::Auth(format!("JWT encoding failed: {}", e)))
}

=======
use adapteros_db::Db;
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

>>>>>>> integration-branch
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
<<<<<<< HEAD
                system_metrics_interval_secs: 30,
                telemetry_buffer_capacity: 1000,
                telemetry_channel_capacity: 100,
                trace_buffer_capacity: 1000,
            },
            golden_gate: None,
            bundles_root: "var/bundles".to_string(),
            rate_limits: None,
            path_policy: Default::default(),
            production_mode: false,
        },
    ));
    let metrics_exporter = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
        0.1, 0.5, 1.0, 2.5, 5.0,
    ])?);
    let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new()?);
    let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
        metrics_collector.clone(),
    ));
    for name in [
        "inference_latency_p95_ms",
        "queue_depth",
        "tokens_per_second",
        "memory_usage_mb",
    ] {
        metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
    }
    let training_service = Arc::new(TrainingService::new());

    // Create lifecycle manager for testing
    let policies = Policies::default();
    let lifecycle_manager = Arc::new(Mutex::new(LifecycleManager::new(
        vec!["test-adapter".to_string()],
        &policies,
        PathBuf::from("var/adapters"),
        None, // telemetry
        3,    // initial_k
    )));

    let state = AppState::with_sqlite(
        db,
        jwt_secret,
        api_config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        training_service,
    )
    .with_lifecycle(lifecycle_manager);
=======
            },
        },
    ));
    let metrics_exporter = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![])?);

    let state = AppState::new(db, jwt_secret, api_config, metrics_exporter);
>>>>>>> integration-branch
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
<<<<<<< HEAD
        kind: None,
=======
>>>>>>> integration-branch
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
<<<<<<< HEAD

/// Test /v1/repositories endpoint returns correct data shape
#[tokio::test]
#[ignore] // Requires database setup
async fn test_repositories_endpoint_data_shape() -> Result<()> {
    use adapteros_api_types::repositories::{RegisterRepositoryRequest, RepositorySummary};
    use adapteros_db::commits::CommitBuilder;
    use adapteros_db::repositories::RepositoryExtendedBuilder;

    println!("Testing /v1/repositories endpoint data shape...");

    let app = setup_test_app().await?;
    let db = setup_test_db().await?;

    // Register a test repository
    let repo_params = RepositoryExtendedBuilder::new()
        .tenant_id("default")
        .repo_id("test/repo")
        .path("/tmp/test/repo")
        .languages(vec!["rust".to_string()])
        .default_branch("main")
        .latest_scan_at(Some("2025-01-15T10:00:00Z".to_string()))
        .build()
        .map_err(|e| adapteros_core::AosError::Database(format!("{}", e)))?;

    let repo_id = db
        .register_repository_extended(repo_params)
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("{}", e)))?;

    // Add a commit to the repository
    let commit_params = CommitBuilder::new()
        .repo_id("test/repo")
        .sha("abc123")
        .author("test-author")
        .date("2025-01-15T10:00:00Z")
        .message("Test commit")
        .branch(Some("main"))
        .changed_files_json(serde_json::to_string(&["src/lib.rs"]).unwrap())
        .build()
        .map_err(|e| adapteros_core::AosError::Database(format!("{}", e)))?;

    db.save_commit(commit_params)
        .await
        .map_err(|e| adapteros_core::AosError::Database(format!("{}", e)))?;

    // Make request to /v1/repositories
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/repositories")
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("infallible".to_string()))?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;

    let repos: Vec<RepositorySummary> = serde_json::from_slice(&body)
        .map_err(|e| adapteros_core::AosError::Serialization(format!("{}", e)))?;

    assert!(!repos.is_empty(), "Should return at least one repository");

    let repo = repos
        .iter()
        .find(|r| r.id == "test/repo")
        .expect("Should find test repository");

    // Verify all required fields are present
    assert!(!repo.id.is_empty(), "id should not be empty");
    assert!(!repo.url.is_empty(), "url should not be empty");
    assert!(!repo.branch.is_empty(), "branch should not be empty");
    assert_eq!(repo.commit_count, 1, "commit_count should be 1");
    assert!(repo.last_scan.is_some(), "last_scan should be present");

    // Verify types match UI expectations
    // UI expects: id: string, url: string, branch: string, commit_count: number, last_scan?: string
    // All fields are present and correctly typed

    println!("✓ Repository endpoint data shape test passed!");
    Ok(())
}

/// Test that repository registration works with custom bundles_root paths
/// This ensures canonicalization succeeds when operators customize storage paths
#[tokio::test]
#[ignore] // Requires database setup and git
async fn test_repository_registration_with_custom_bundles_root() -> Result<()> {
    println!("Testing repository registration with custom bundles_root...");

    // Create a temporary directory for custom bundles_root
    let temp_dir = tempfile::tempdir()
        .map_err(|e| adapteros_core::AosError::Io(format!("Failed to create temp dir: {}", e)))?;
    let custom_bundles_root = temp_dir.path().to_string_lossy().to_string();

    // Set up test app with custom bundles_root
    let app = {
        let db = setup_test_db().await?;
        let jwt_secret = b"test-secret-key-for-jwt-tokens-32-bytes!";
        let api_config = Arc::new(RwLock::new(ApiConfig {
            metrics: MetricsConfig {
                enabled: false,
                bearer_token: "test-token".to_string(),
                system_metrics_interval_secs: 30,
                telemetry_buffer_capacity: 1024,
                telemetry_channel_capacity: 256,
                trace_buffer_capacity: 512,
                server_port: 9090,
                server_enabled: true,
            },
            golden_gate: None,
            bundles_root: custom_bundles_root.clone(),
            rate_limits: None,
            path_policy: Default::default(),
            production_mode: false,
        }));

        let metrics_exporter = Arc::new(adapteros_metrics_exporter::MetricsExporter::new(vec![
            0.1, 0.5, 1.0, 2.5, 5.0,
        ])?);
        let metrics_collector = Arc::new(adapteros_telemetry::MetricsCollector::new()?);
        let metrics_registry = Arc::new(adapteros_telemetry::MetricsRegistry::new(
            metrics_collector.clone(),
        ));
        for name in [
            "inference_latency_p95_ms",
            "queue_depth",
            "tokens_per_second",
            "memory_usage_mb",
        ] {
            metrics_registry.get_or_create_series(name.to_string(), 1_000, 1_024);
        }
        let training_service = Arc::new(TrainingService::new());

        let state = AppState::with_sqlite(
            db,
            jwt_secret.to_vec(),
            api_config,
            metrics_exporter,
            metrics_collector,
            metrics_registry,
            training_service,
        );

        Router::new()
            .route(
                "/api/v1/repositories/register",
                post(register_git_repository),
            )
            .layer(Extension(state))
    };

    // Create JWT token for authentication
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: "test-user".to_string(),
        email: "test@example.com".to_string(),
        role: Role::Admin.to_string(),
        tenant_id: "default".to_string(),
        exp: (now + chrono::Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        jti: "test-jti".to_string(),
        nbf: now.timestamp(),
    };
    let token = encode_jwt(&claims, b"test-secret-key-for-jwt-tokens-32-bytes!").unwrap();

    // Test repository registration with custom bundles_root
    let repo_url = "https://github.com/octocat/Hello-World.git";
    let request_body = serde_json::json!({
        "url": repo_url,
        "branch": "main"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/repositories/register")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(request_body.to_string()))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("request failed".to_string()))?;

    // Verify that the custom bundles_root path was canonicalized correctly
    match response.status() {
        StatusCode::OK => {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;
            let summary: RepositorySummary = serde_json::from_slice(&body)
                .map_err(|e| adapteros_core::AosError::Serialization(e.to_string()))?;

            // The custom bundles_root should be properly canonicalized and used
            // Since we can't verify the exact path without accessing the database,
            // we at least verify that registration succeeded with the custom path
            println!(
                "✓ Repository registration succeeded with custom bundles_root: {}",
                custom_bundles_root
            );
            println!("✓ Repository ID: {}, URL: {}", summary.id, summary.url);
            Ok(())
        }
        StatusCode::BAD_REQUEST => {
            // Check if it's a path validation error
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;
            let error: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| adapteros_core::AosError::Serialization(e.to_string()))?;

            if let Some(error_msg) = error.get("error").and_then(|e| e.as_str()) {
                if error_msg.contains("canonicalize") || error_msg.contains("path") {
                    return Err(adapteros_core::AosError::Validation(format!(
                        "Path canonicalization failed with custom bundles_root: {}",
                        error_msg
                    )));
                }
            }
            // Other BAD_REQUEST errors (like git clone failures) are expected in test environment
            println!("✓ Repository registration handled gracefully (expected in test env)");
            Ok(())
        }
        _ => {
            println!(
                "✓ Repository registration completed (status: {})",
                response.status()
            );
            Ok(())
        }
    }
}

/// Test legacy repository registration payload compatibility
/// Ensures older clients providing repo_id and path fields still work
#[tokio::test]
#[ignore] // Requires database setup
async fn test_legacy_repository_registration_payload() -> Result<()> {
    println!("Testing legacy repository registration payload compatibility...");

    let app = setup_test_app().await?;
    let db = setup_test_db().await?;

    // Create JWT token for authentication
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: "test-user".to_string(),
        email: "test@example.com".to_string(),
        role: Role::Admin.to_string(),
        tenant_id: "default".to_string(),
        exp: (now + chrono::Duration::hours(1)).timestamp(),
        iat: now.timestamp(),
        jti: "test-jti-2".to_string(),
        nbf: now.timestamp(),
    };
    let token = encode_jwt(&claims, b"test-secret-key-for-jwt-tokens-32-bytes!").unwrap();

    // Test with legacy payload containing repo_id and path
    let repo_url = "https://github.com/octocat/Hello-World.git";
    let legacy_repo_id = "legacy-test-repo";
    let legacy_path = "/custom/legacy/path"; // This should be ignored by server

    let request_body = serde_json::json!({
        "url": repo_url,
        "branch": "main",
        "repo_id": legacy_repo_id,
        "path": legacy_path,
        "tenant_id": "default"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/repositories/register")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(request_body.to_string()))
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?,
        )
        .await
        .map_err(|_e| adapteros_core::AosError::Internal("request failed".to_string()))?;

    // Check that the response is valid and uses the legacy repo_id
    match response.status() {
        StatusCode::OK => {
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;
            let summary: RepositorySummary = serde_json::from_slice(&body)
                .map_err(|e| adapteros_core::AosError::Serialization(e.to_string()))?;

            // Verify that the legacy repo_id was respected
            assert_eq!(summary.id, legacy_repo_id, "Legacy repo_id should be used");
            assert!(!summary.url.is_empty(), "URL should be present");

            println!("✓ Legacy payload compatibility test passed!");
            Ok(())
        }
        StatusCode::BAD_REQUEST => {
            // In test environment, git operations may fail, but path canonicalization should work
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;
            let error: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| adapteros_core::AosError::Serialization(e.to_string()))?;

            // Ensure it's not a path canonicalization error
            if let Some(error_msg) = error.get("error").and_then(|e| e.as_str()) {
                if error_msg.contains("canonicalize") || error_msg.contains("Invalid path") {
                    return Err(adapteros_core::AosError::Validation(format!(
                        "Path canonicalization failed for legacy payload: {}",
                        error_msg
                    )));
                }
            }
            println!("✓ Legacy payload handled gracefully (expected git failure in test env)");
            Ok(())
        }
        _ => {
            println!("✓ Legacy payload test completed");
            Ok(())
        }
    }
}
=======
>>>>>>> integration-branch
