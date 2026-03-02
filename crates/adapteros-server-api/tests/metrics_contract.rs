use adapteros_core::error_codes;
use adapteros_server_api::handlers::{inference_metrics, metrics, metrics_handler};
use axum::http::Request;
use axum::{body, extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde_json::Value;
use tower::util::ServiceExt;
use utoipa::OpenApi;

mod common;

#[test]
fn metrics_openapi_documents_inference_and_system_paths() {
    let spec = adapteros_server_api::routes::ApiDoc::openapi();
    let value: Value = serde_json::to_value(spec).expect("generated OpenAPI should serialize");
    let paths = value
        .pointer("/paths")
        .and_then(Value::as_object)
        .expect("OpenAPI paths should be present");

    assert!(
        paths.contains_key("/v1/metrics/inference"),
        "metrics inference route must be in OpenAPI"
    );
    assert!(
        paths.contains_key("/v1/metrics/system"),
        "metrics system compatibility route must remain in OpenAPI"
    );
}

#[tokio::test]
async fn metrics_router_exposes_inference_endpoint() {
    let state = common::setup_state(None).await.expect("state");
    let app = adapteros_server_api::create_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/metrics/inference")
                .body(axum::body::Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "/v1/metrics/inference must be routed"
    );
}

#[tokio::test]
async fn system_metrics_fail_closed_when_request_log_missing() -> anyhow::Result<()> {
    let state = common::setup_state(None).await.expect("state");
    sqlx::query("DROP TABLE request_log")
        .execute(state.db.pool_result()?)
        .await
        .expect("request_log should drop");

    let claims = common::test_admin_claims();
    let err = metrics::get_system_metrics(State(state), Extension(claims))
        .await
        .expect_err("missing request_log must fail closed");

    let (status, Json(body)) = err;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.code, error_codes::SCHEMA_CONTRACT_VIOLATION);
    Ok(())
}

#[tokio::test]
async fn system_metrics_succeeds_when_request_log_has_no_recent_rows() -> anyhow::Result<()> {
    let state = common::setup_state(None).await.expect("state");
    sqlx::query("DELETE FROM request_log")
        .execute(state.db.pool_result()?)
        .await
        .expect("request_log should clear");

    let claims = common::test_admin_claims();
    let result = metrics::get_system_metrics(State(state), Extension(claims)).await;
    assert!(
        result.is_ok(),
        "empty request_log window must not fail /v1/metrics/system"
    );
    Ok(())
}

#[tokio::test]
async fn inference_metrics_fail_closed_when_request_log_missing() -> anyhow::Result<()> {
    let state = common::setup_state(None).await.expect("state");
    sqlx::query("DROP TABLE request_log")
        .execute(state.db.pool_result()?)
        .await
        .expect("request_log should drop");

    let claims = common::test_admin_claims();
    let err = inference_metrics::get_inference_metrics_handler(State(state), Extension(claims))
        .await
        .expect_err("missing request_log must fail closed");

    let (status, Json(body)) = err;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.code, error_codes::SCHEMA_CONTRACT_VIOLATION);
    Ok(())
}

#[tokio::test]
async fn metrics_endpoint_exports_phase5_observability_contract() -> anyhow::Result<()> {
    let state = common::setup_state(None).await.expect("state");

    state
        .metrics_exporter
        .record_inference_request("tenant-1", "qwen-7b", "success", 0.2, 64);
    state
        .metrics_exporter
        .record_model_load("qwen-7b", "tenant-1", true);
    state
        .metrics_exporter
        .record_model_load_duration("qwen-7b", "tenant-1", 1.5);
    state
        .metrics_exporter
        .set_memory_pressure_ratio("system", 0.42);
    state
        .metrics_exporter
        .set_memory_pressure_ratio("heap", 0.31);

    let response = metrics_handler(State(state)).await.into_response();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("metrics body");
    let payload = String::from_utf8(bytes.to_vec()).expect("utf8 metrics");

    assert!(payload.contains("aos_inference_ttft_seconds_bucket"));
    assert!(payload.contains("aos_inference_tps_bucket"));
    assert!(payload.contains("adapteros_model_load_duration_seconds_bucket"));
    assert!(payload.contains("aos_memory_pressure_ratio{pool_type=\"system\""));
    assert!(payload.contains("aos_memory_pressure_ratio{pool_type=\"heap\""));
    Ok(())
}
