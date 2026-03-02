//! OpenAI-compatible Models list endpoint tests
//!
//! Tests for GET /v1/models verifying OpenAI-compatible response format.
//!
//! [2026-01-29 openai_models_list_test]

mod common;

use adapteros_server_api::handlers::openai_compat::OpenAiModelListResponse;
use adapteros_server_api::routes;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// Verify that GET /v1/models returns OpenAI-compatible format.
///
/// Expected format:
/// ```json
/// {
///   "object": "list",
///   "data": [
///     { "id": "...", "object": "model", "created": 123, "owned_by": "..." }
///   ]
/// }
/// ```
#[tokio::test]
async fn test_v1_models_returns_openai_format() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup_state");

    // Register a test model with all required NOT NULL fields
    adapteros_db::sqlx::query(
        r#"
        INSERT OR REPLACE INTO models (
            id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
            format, backend, size_bytes, import_status, model_path,
            tenant_id, imported_at, updated_at
        ) VALUES (
            'test-model-1', 'Test Model 1', 'hash1', 'config1', 'tok1', 'tok_cfg1',
            'mlx', 'mlx', 1000000, 'available', '/var/models/test',
            'default', '2024-06-16T12:30:02Z', '2024-06-16T12:30:02Z'
        )
        "#,
    )
    .execute(state.db.pool())
    .await
    .expect("insert test model");

    let app = routes::build(state.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/v1/models")
        .header("content-type", "application/json")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "expected 200 OK for /v1/models"
    );

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Parse and verify OpenAI format
    let parsed: OpenAiModelListResponse =
        serde_json::from_slice(&body).expect("parse OpenAI response");

    assert_eq!(parsed.object, "list", "root object must be 'list'");
    assert!(!parsed.data.is_empty(), "data array should not be empty");

    // Verify the model we inserted is in the response
    let model = parsed
        .data
        .iter()
        .find(|m| m.id == "test-model-1")
        .expect("find test model in response");

    assert_eq!(model.object, "model", "model object must be 'model'");
    assert!(model.created > 0, "created timestamp should be positive");
    assert_eq!(model.owned_by, "default", "owned_by should match tenant_id");

    // Also verify JSON structure directly
    let json: serde_json::Value = serde_json::from_str(&body_str).expect("parse as Value");
    assert!(json.get("object").is_some(), "must have object field");
    assert!(json.get("data").is_some(), "must have data field");
    assert!(
        json.get("models").is_none(),
        "should NOT have legacy 'models' field"
    );
    assert!(
        json.get("total").is_none(),
        "should NOT have legacy 'total' field"
    );
}

/// Verify that the internal endpoint still provides full AdapterOS model details.
#[tokio::test]
async fn test_internal_models_returns_full_details() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup_state");

    // Register a test model with extra fields (including all required NOT NULL fields)
    adapteros_db::sqlx::query(
        r#"
        INSERT OR REPLACE INTO models (
            id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
            format, backend, size_bytes, import_status, model_path,
            capabilities, quantization, tenant_id, imported_at, updated_at
        ) VALUES (
            'detailed-model', 'Detailed Model', 'hash2', 'config2', 'tok2', 'tok_cfg2',
            'mlx', 'mlx', 5000000000, 'available', '/var/models/detailed',
            '["inference","training"]', 'q4', 'default',
            '2024-06-16T12:30:02Z', '2024-06-16T12:30:02Z'
        )
        "#,
    )
    .execute(state.db.pool())
    .await
    .expect("insert detailed model");

    let app = routes::build(state.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/internal/models")
        .header("content-type", "application/json")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("response");

    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "expected 200 OK for /internal/models"
    );

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body_str = String::from_utf8_lossy(&body);

    // Parse and verify AdapterOS format
    let json: serde_json::Value = serde_json::from_str(&body_str).expect("parse JSON");

    // Internal format should have 'models' and 'total'
    assert!(
        json.get("models").is_some(),
        "internal endpoint must have 'models' field"
    );
    assert!(
        json.get("total").is_some(),
        "internal endpoint must have 'total' field"
    );

    // Verify detailed fields are present
    let models = json["models"].as_array().expect("models is array");
    let model = models
        .iter()
        .find(|m| m["id"] == "detailed-model")
        .expect("find detailed model");

    assert!(model.get("size_bytes").is_some(), "should have size_bytes");
    assert!(
        model.get("capabilities").is_some(),
        "should have capabilities"
    );
    assert!(
        model.get("quantization").is_some(),
        "should have quantization"
    );
    assert!(
        model.get("adapter_count").is_some(),
        "should have adapter_count"
    );
    assert!(
        model.get("training_job_count").is_some(),
        "should have training_job_count"
    );
}

/// Verify that the OpenAI format timestamp is correctly parsed from imported_at.
#[tokio::test]
async fn test_openai_model_created_timestamp() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup_state");

    // Use a known Unix timestamp to avoid timezone issues
    // 1718537402 = 2024-06-16T12:30:02+00:00
    let known_timestamp: i64 = 1718537402;
    let imported_at = chrono::DateTime::from_timestamp(known_timestamp, 0)
        .expect("valid timestamp")
        .to_rfc3339();

    // Register a model with the computed timestamp
    adapteros_db::sqlx::query(
        r#"
        INSERT OR REPLACE INTO models (
            id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
            format, import_status, tenant_id,
            imported_at, updated_at
        ) VALUES (
            'timestamp-model', 'Timestamp Model', 'hash3', 'config3', 'tok3', 'tok_cfg3',
            'mlx', 'available', 'default',
            ?1, ?1
        )
        "#,
    )
    .bind(&imported_at)
    .execute(state.db.pool())
    .await
    .expect("insert timestamp model");

    let app = routes::build(state.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/v1/models")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("response");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    let parsed: OpenAiModelListResponse = serde_json::from_slice(&body).expect("parse response");

    let model = parsed
        .data
        .iter()
        .find(|m| m.id == "timestamp-model")
        .expect("find timestamp model");

    // Verify the timestamp matches what we inserted
    assert_eq!(
        model.created, known_timestamp,
        "created should be Unix timestamp of imported_at"
    );
}

/// Verify owned_by defaults to "adapteros" when tenant_id is null.
#[tokio::test]
async fn test_openai_model_owned_by_default() {
    let _guard = common::TestkitEnvGuard::enabled(true).await;

    let state = common::setup_state(None).await.expect("setup_state");

    // Register a model without tenant_id (using NULL)
    adapteros_db::sqlx::query(
        r#"
        INSERT OR REPLACE INTO models (
            id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
            format, import_status, tenant_id,
            imported_at, updated_at
        ) VALUES (
            'no-tenant-model', 'No Tenant Model', 'hash4', 'config4', 'tok4', 'tok_cfg4',
            'mlx', 'available', NULL,
            '2024-06-16T12:30:02Z', '2024-06-16T12:30:02Z'
        )
        "#,
    )
    .execute(state.db.pool())
    .await
    .expect("insert no-tenant model");

    let app = routes::build(state.clone());

    let req = Request::builder()
        .method("GET")
        .uri("/v1/models")
        .body(Body::empty())
        .expect("build request");

    let response = app.oneshot(req).await.expect("response");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();

    let parsed: OpenAiModelListResponse = serde_json::from_slice(&body).expect("parse response");

    let model = parsed
        .data
        .iter()
        .find(|m| m.id == "no-tenant-model")
        .expect("find no-tenant model");

    assert_eq!(
        model.owned_by, "adapteros",
        "owned_by should default to 'adapteros' when tenant_id is null"
    );
}
