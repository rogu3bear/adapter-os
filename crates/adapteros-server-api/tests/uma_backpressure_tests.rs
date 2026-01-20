use adapteros_api_types::InferRequest;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use adapteros_server_api::handlers::inference::infer;
use adapteros_server_api::middleware::request_id::RequestId;
use axum::{extract::State, http::StatusCode, Extension, Json};

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn uma_backpressure_short_circuits_inference() {
    let state = setup_state(None).await.expect("state");
    state
        .uma_monitor
        .set_pressure_for_test(MemoryPressureLevel::Critical);

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test".to_string(),
    );

    let req = InferRequest {
        prompt: "hello backpressure".to_string(),
        ..Default::default()
    };

    let err = infer(
        State(state.clone()),
        Extension(claims),
        Extension(identity),
        Some(Extension(RequestId("backpressure".to_string()))),
        None,
        Json(req),
    )
    .await
    .expect_err("should reject under UMA backpressure");

    assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(err.code, "BACKPRESSURE");
    let details = err.details.clone().expect("details payload");
    assert_eq!(details["level"], "Critical");
    assert_eq!(details["retry_after_secs"], 30);
}
