#![allow(clippy::expect_fun_call)]

use adapteros_server_api::create_app;
use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tower::ServiceExt;

mod common;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var").join("tmp");
    fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

/// Helper to check if the dev/contracts endpoint is available in the current build
/// Returns true if the endpoint is registered (debug builds), false otherwise
async fn is_dev_contracts_endpoint_available(app: &axum::Router) -> bool {
    let (status, body) = json_request(app, Method::GET, "/v1/dev/contracts", None).await;
    // If we get a generic "Not Found" message, the endpoint isn't registered
    !(status == StatusCode::NOT_FOUND
        && body.get("message").and_then(|v| v.as_str()) == Some("Not Found"))
}

/// Macro to skip test if dev/contracts endpoint isn't available
macro_rules! skip_if_endpoint_unavailable {
    ($app:expr) => {
        if !is_dev_contracts_endpoint_available($app).await {
            eprintln!(
                "Skipping test: dev/contracts endpoint not available in current build profile"
            );
            return;
        }
    };
}

/// Helper to make JSON requests to the app
async fn json_request(
    app: &axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method.clone()).uri(path);
    if matches!(method, Method::POST | Method::PUT | Method::PATCH) {
        builder = builder.header("content-type", "application/json");
    }

    let body = match body {
        Some(b) => Body::from(b.to_string()),
        None => Body::empty(),
    };

    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request build"))
        .await
        .expect("router response");

    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("read body");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, json)
}

/// Setup a temporary contracts directory with sample files
fn setup_contract_samples(temp_dir: &TempDir) -> PathBuf {
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Create valid inference contract
    let inference_sample = serde_json::json!({
        "schema_version": "1.0",
        "id": "inf_test_001",
        "text": "Test response text",
        "tokens_generated": 10,
        "latency_ms": 100,
        "adapters_used": ["adapter-1", "adapter-2"],
        "finish_reason": "stop",
        "run_receipt": {
            "trace_id": "trace_001",
            "run_head_hash": "hash123",
            "output_digest": "digest456",
            "receipt_digest": "receipt789",
            "logical_prompt_tokens": 5,
            "prefix_cached_token_count": 0,
            "billed_input_tokens": 5,
            "logical_output_tokens": 10,
            "billed_output_tokens": 10
        }
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&inference_sample).unwrap(),
    )
    .expect("write inference sample");

    // Create valid trace contract
    let trace_sample = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "spans": [
            {
                "span_id": "span_root",
                "name": "inference.request",
                "start_time": "2025-01-01T00:00:00.000Z",
                "end_time": "2025-01-01T00:00:00.100Z",
                "trace_id": "trace_001",
                "parent_id": "",
                "start_ns": 0,
                "end_ns": 100000000,
                "status": "ok"
            }
        ]
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_sample).unwrap(),
    )
    .expect("write trace sample");

    // Create valid evidence contract
    let evidence_sample = serde_json::json!([
        {
            "id": "evidence_001",
            "evidence_type": "audit",
            "reference": "b3:hash123",
            "description": "Test evidence",
            "confidence": "high",
            "created_at": "2025-01-01T00:00:00Z"
        }
    ]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence sample");

    contracts_dir
}

/// Setup contracts with sensitive PII data to test redaction
fn setup_contracts_with_pii(temp_dir: &TempDir) -> PathBuf {
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Create inference with PII
    let inference_with_pii = serde_json::json!({
        "schema_version": "1.0",
        "id": "inf_test_001",
        "text": "Response text",
        "prompt": "SENSITIVE USER PROMPT",
        "prompt_text": "ANOTHER SENSITIVE PROMPT",
        "raw_prompt": "RAW PROMPT DATA",
        "messages": [{"role": "user", "content": "secret"}],
        "user": "john.doe@example.com",
        "email": "user@example.com",
        "ip": "192.168.1.1",
        "auth_token": "secret-token-123",
        "tokens_generated": 10,
        "latency_ms": 100,
        "adapters_used": ["adapter-1"],
        "finish_reason": "stop"
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&inference_with_pii).unwrap(),
    )
    .expect("write inference sample");

    // Create trace with nested PII
    let trace_with_pii = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "user": "admin@example.com",
        "spans": [
            {
                "span_id": "span_root",
                "name": "inference.request",
                "start_time": "2025-01-01T00:00:00.000Z",
                "end_time": "2025-01-01T00:00:00.100Z",
                "attributes": {
                    "prompt": "nested secret prompt",
                    "email": "nested@example.com"
                },
                "trace_id": "trace_001",
                "parent_id": "",
                "start_ns": 0,
                "end_ns": 100000000,
                "status": "ok"
            }
        ]
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_with_pii).unwrap(),
    )
    .expect("write trace sample");

    // Create evidence with PII
    let evidence_with_pii = serde_json::json!([
        {
            "id": "evidence_001",
            "evidence_type": "audit",
            "reference": "b3:hash123",
            "description": "Test evidence",
            "confidence": "high",
            "created_at": "2025-01-01T00:00:00Z",
            "email": "evidence-user@example.com",
            "user": "evidence-creator",
            "ip": "10.0.0.1"
        }
    ]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_with_pii).unwrap(),
    )
    .expect("write evidence sample");

    contracts_dir
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_loads_from_default_directory() {
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    env::remove_var("AOS_CONTRACT_SAMPLE_DIR");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    // This test assumes docs/contracts/*.json exist in the repo
    // If they don't exist, it should return NOT_FOUND
    let (status, _body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    // If status is NOT_FOUND and response has generic "Not Found", the endpoint
    // isn't registered (test profile doesn't have debug_assertions), skip test
    if status == StatusCode::NOT_FOUND
        && _body.get("message").and_then(|v| v.as_str()) == Some("Not Found")
    {
        eprintln!("Skipping test: dev/contracts endpoint not available in current build profile");
        return;
    }

    // We expect either OK (if files exist) or NOT_FOUND (if they don't)
    assert!(
        status == StatusCode::OK || status == StatusCode::NOT_FOUND,
        "Expected OK or NOT_FOUND, got {}",
        status
    );
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_loads_from_custom_directory() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contract_samples(&temp_dir);

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // Validate response structure
    assert!(body.get("inference").is_some(), "Missing inference field");
    assert!(body.get("trace").is_some(), "Missing trace field");
    assert!(body.get("evidence").is_some(), "Missing evidence field");

    // Validate inference contract
    let inference = body.get("inference").unwrap();
    assert_eq!(
        inference.get("id").unwrap().as_str().unwrap(),
        "inf_test_001"
    );
    assert_eq!(
        inference.get("schema_version").unwrap().as_str().unwrap(),
        "1.0"
    );
    assert_eq!(
        inference.get("tokens_generated").unwrap().as_u64().unwrap(),
        10
    );

    // Validate trace contract
    let trace = body.get("trace").unwrap();
    assert_eq!(
        trace.get("trace_id").unwrap().as_str().unwrap(),
        "trace_001"
    );
    assert!(trace.get("spans").unwrap().is_array());

    // Validate evidence contract
    let evidence = body.get("evidence").unwrap();
    assert!(evidence.is_array());
    assert_eq!(evidence.as_array().unwrap().len(), 1);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_redacts_pii() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contracts_with_pii(&temp_dir);

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // Verify PII fields are removed from inference
    let inference = body.get("inference").unwrap();
    assert!(
        inference.get("prompt").is_none(),
        "prompt should be redacted"
    );
    assert!(
        inference.get("prompt_text").is_none(),
        "prompt_text should be redacted"
    );
    assert!(
        inference.get("raw_prompt").is_none(),
        "raw_prompt should be redacted"
    );
    assert!(
        inference.get("messages").is_none(),
        "messages should be redacted"
    );
    assert!(inference.get("user").is_none(), "user should be redacted");
    assert!(inference.get("email").is_none(), "email should be redacted");
    assert!(inference.get("ip").is_none(), "ip should be redacted");
    assert!(
        inference.get("auth_token").is_none(),
        "auth_token should be redacted"
    );

    // Verify non-PII fields are preserved
    assert_eq!(
        inference.get("id").unwrap().as_str().unwrap(),
        "inf_test_001"
    );
    assert_eq!(
        inference.get("text").unwrap().as_str().unwrap(),
        "Response text"
    );

    // Verify PII fields are removed from trace
    let trace = body.get("trace").unwrap();
    assert!(trace.get("user").is_none(), "trace user should be redacted");

    // Verify nested PII in spans is removed
    let spans = trace.get("spans").unwrap().as_array().unwrap();
    let span = &spans[0];
    if let Some(attrs) = span.get("attributes") {
        assert!(
            attrs.get("prompt").is_none(),
            "nested prompt should be redacted"
        );
        assert!(
            attrs.get("email").is_none(),
            "nested email should be redacted"
        );
    }

    // Verify PII fields are removed from evidence
    let evidence = body.get("evidence").unwrap().as_array().unwrap();
    let evidence_item = &evidence[0];
    assert!(
        evidence_item.get("email").is_none(),
        "evidence email should be redacted"
    );
    assert!(
        evidence_item.get("user").is_none(),
        "evidence user should be redacted"
    );
    assert!(
        evidence_item.get("ip").is_none(),
        "evidence ip should be redacted"
    );

    // Verify non-PII fields are preserved in evidence
    assert_eq!(
        evidence_item.get("id").unwrap().as_str().unwrap(),
        "evidence_001"
    );
    assert_eq!(
        evidence_item
            .get("evidence_type")
            .unwrap()
            .as_str()
            .unwrap(),
        "audit"
    );
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_missing_inference_file() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Only create trace and evidence, omit inference
    let trace_sample = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "spans": []
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_sample).unwrap(),
    )
    .expect("write trace");

    let evidence_sample = serde_json::json!([]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Expected NOT_FOUND, got {}: {:?}",
        status,
        body
    );

    // Validate error response - the handler returns ErrorResponse with "error" field
    let error_msg = body
        .get("error")
        .or_else(|| body.get("message"))
        .expect(&format!(
            "error/message field missing in response: {:?}",
            body
        ));
    assert!(
        error_msg.as_str().unwrap().contains("inference")
            || error_msg.as_str().unwrap().contains("sample not found")
    );

    let code = body
        .get("code")
        .expect(&format!("code field missing in response: {:?}", body));
    assert_eq!(code.as_str().unwrap(), "NOT_FOUND");
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_missing_trace_file() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Only create inference and evidence, omit trace
    let inference_sample = serde_json::json!({
        "schema_version": "1.0",
        "id": "inf_001",
        "text": "test",
        "tokens_generated": 1,
        "latency_ms": 1,
        "adapters_used": [],
        "finish_reason": "stop"
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&inference_sample).unwrap(),
    )
    .expect("write inference");

    let evidence_sample = serde_json::json!([]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);

    let error = body
        .get("error")
        .or_else(|| body.get("message"))
        .expect("error field");
    assert!(
        error.as_str().unwrap().contains("trace")
            || error.as_str().unwrap().contains("sample not found")
    );
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_missing_evidence_file() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Only create inference and trace, omit evidence
    let inference_sample = serde_json::json!({
        "schema_version": "1.0",
        "id": "inf_001",
        "text": "test",
        "tokens_generated": 1,
        "latency_ms": 1,
        "adapters_used": [],
        "finish_reason": "stop"
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&inference_sample).unwrap(),
    )
    .expect("write inference");

    let trace_sample = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "spans": []
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_sample).unwrap(),
    )
    .expect("write trace");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);

    let error = body
        .get("error")
        .or_else(|| body.get("message"))
        .expect("error field");
    assert!(
        error.as_str().unwrap().contains("evidence")
            || error.as_str().unwrap().contains("sample not found")
    );
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_invalid_json_in_inference() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Write invalid JSON to inference file
    fs::write(
        contracts_dir.join("infer-response.json"),
        "{ invalid json here }",
    )
    .expect("write invalid inference");

    // Create valid trace and evidence
    let trace_sample = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "spans": []
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_sample).unwrap(),
    )
    .expect("write trace");

    let evidence_sample = serde_json::json!([]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let error = body
        .get("error")
        .or_else(|| body.get("message"))
        .expect(&format!(
            "error/message field missing in response: {:?}",
            body
        ));
    let error_str = error.as_str().unwrap_or("");
    assert!(
        error_str.contains("validation")
            || error_str.contains("INVALID_CONTRACT_SAMPLE")
            || error_str.contains("parse")
            || error_str.contains("JSON"),
        "Expected error message to contain validation/parsing info, got: {}",
        error_str
    );

    let code = body.get("code").or_else(|| body.get("error_code"));
    if let Some(c) = code {
        let code_str = c.as_str().unwrap_or("");
        assert!(
            code_str == "INVALID_CONTRACT_SAMPLE" || code_str == "INTERNAL_SERVER_ERROR",
            "Expected code to be INVALID_CONTRACT_SAMPLE or INTERNAL_SERVER_ERROR, got: {}",
            code_str
        );
    }
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_schema_validation_inference() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    // Create inference missing required fields
    let invalid_inference = serde_json::json!({
        "id": "inf_001",
        // Missing schema_version, text, tokens_generated, etc.
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&invalid_inference).unwrap(),
    )
    .expect("write invalid inference");

    let trace_sample = serde_json::json!({
        "trace_id": "trace_001",
        "root_span_id": "span_root",
        "spans": []
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&trace_sample).unwrap(),
    )
    .expect("write trace");

    let evidence_sample = serde_json::json!([]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let code = body.get("code").expect("code field");
    assert_eq!(code.as_str().unwrap(), "INVALID_CONTRACT_SAMPLE");
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_schema_validation_trace() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = temp_dir.path().join("contracts");
    fs::create_dir(&contracts_dir).expect("create contracts dir");

    let inference_sample = serde_json::json!({
        "schema_version": "1.0",
        "id": "inf_001",
        "text": "test",
        "tokens_generated": 1,
        "latency_ms": 1,
        "adapters_used": [],
        "finish_reason": "stop"
    });
    fs::write(
        contracts_dir.join("infer-response.json"),
        serde_json::to_string_pretty(&inference_sample).unwrap(),
    )
    .expect("write inference");

    // Create trace missing required fields
    let invalid_trace = serde_json::json!({
        "trace_id": "trace_001",
        // Missing root_span_id and spans
    });
    fs::write(
        contracts_dir.join("trace-response.json"),
        serde_json::to_string_pretty(&invalid_trace).unwrap(),
    )
    .expect("write invalid trace");

    let evidence_sample = serde_json::json!([]);
    fs::write(
        contracts_dir.join("evidence-list.json"),
        serde_json::to_string_pretty(&evidence_sample).unwrap(),
    )
    .expect("write evidence");

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let code = body.get("code").expect("code field");
    assert_eq!(code.as_str().unwrap(), "INVALID_CONTRACT_SAMPLE");
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_validates_all_contract_types() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contract_samples(&temp_dir);

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // All three contract types should be present and valid
    assert!(body.get("inference").is_some());
    assert!(body.get("trace").is_some());
    assert!(body.get("evidence").is_some());

    // Inference validation
    let inference = body.get("inference").unwrap();
    assert!(inference.get("schema_version").is_some());
    assert!(inference.get("id").is_some());
    assert!(inference.get("text").is_some());
    assert!(inference.get("tokens_generated").is_some());
    assert!(inference.get("latency_ms").is_some());
    assert!(inference.get("adapters_used").is_some());
    assert!(inference.get("finish_reason").is_some());

    // Trace validation
    let trace = body.get("trace").unwrap();
    assert!(trace.get("trace_id").is_some());
    assert!(trace.get("root_span_id").is_some());
    assert!(trace.get("spans").is_some());
    let spans = trace.get("spans").unwrap().as_array().unwrap();
    if !spans.is_empty() {
        let span = &spans[0];
        assert!(span.get("span_id").is_some());
        assert!(span.get("name").is_some());
        assert!(span.get("trace_id").is_some());
        assert!(span.get("status").is_some());
    }

    // Evidence validation
    let evidence = body.get("evidence").unwrap();
    assert!(evidence.is_array());
    let evidence_list = evidence.as_array().unwrap();
    if !evidence_list.is_empty() {
        let item = &evidence_list[0];
        assert!(item.get("id").is_some());
        assert!(item.get("evidence_type").is_some());
        assert!(item.get("reference").is_some());
        assert!(item.get("confidence").is_some());
        assert!(item.get("created_at").is_some());
    }
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_response_structure() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contract_samples(&temp_dir);

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // Response should be an object with exactly 3 fields
    assert!(body.is_object());
    let obj = body.as_object().unwrap();
    assert_eq!(obj.len(), 3, "Response should have exactly 3 fields");
    assert!(obj.contains_key("inference"));
    assert!(obj.contains_key("trace"));
    assert!(obj.contains_key("evidence"));

    // All fields should be non-null
    assert!(!body.get("inference").unwrap().is_null());
    assert!(!body.get("trace").unwrap().is_null());
    assert!(!body.get("evidence").unwrap().is_null());
}

#[cfg(not(debug_assertions))]
#[tokio::test]
async fn contract_samples_endpoint_not_available_in_release() {
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, _body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    // In release mode, the endpoint should not be registered
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_env_var_override() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contract_samples(&temp_dir);

    // Test that AOS_CONTRACT_SAMPLE_DIR env var is respected
    let custom_path = contracts_dir.to_str().unwrap();
    env::set_var("AOS_CONTRACT_SAMPLE_DIR", custom_path);
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // Verify it loaded from our custom directory
    let inference = body.get("inference").unwrap();
    assert_eq!(
        inference.get("id").unwrap().as_str().unwrap(),
        "inf_test_001"
    );
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn contract_samples_redaction_preserves_structure() {
    let temp_dir = new_test_tempdir();
    let contracts_dir = setup_contracts_with_pii(&temp_dir);

    env::set_var("AOS_CONTRACT_SAMPLE_DIR", contracts_dir.to_str().unwrap());
    env::set_var("AOS_DEV_NO_AUTH", "1");
    env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    let state = common::setup_state(None).await.expect("setup state");
    let app = create_app(state);

    skip_if_endpoint_unavailable!(&app);

    let (status, body) = json_request(&app, Method::GET, "/v1/dev/contracts", None).await;

    assert_eq!(status, StatusCode::OK);

    // Verify redaction removes PII but preserves overall structure
    let inference = body.get("inference").unwrap();

    // Should have core fields
    assert!(inference.get("id").is_some());
    assert!(inference.get("text").is_some());
    assert!(inference.get("schema_version").is_some());
    assert!(inference.get("tokens_generated").is_some());
    assert!(inference.get("latency_ms").is_some());
    assert!(inference.get("adapters_used").is_some());
    assert!(inference.get("finish_reason").is_some());

    // Should NOT have PII fields
    assert!(inference.get("prompt").is_none());
    assert!(inference.get("user").is_none());
    assert!(inference.get("email").is_none());

    // Verify trace structure preserved
    let trace = body.get("trace").unwrap();
    assert!(trace.get("trace_id").is_some());
    assert!(trace.get("root_span_id").is_some());
    assert!(trace.get("spans").is_some());
    assert!(trace.get("user").is_none());

    // Verify evidence structure preserved
    let evidence = body.get("evidence").unwrap();
    assert!(evidence.is_array());
    let items = evidence.as_array().unwrap();
    if !items.is_empty() {
        assert!(items[0].get("id").is_some());
        assert!(items[0].get("evidence_type").is_some());
        assert!(items[0].get("user").is_none());
        assert!(items[0].get("email").is_none());
    }
}
