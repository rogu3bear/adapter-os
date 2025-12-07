use adapteros_server_api::types::InferenceError;
use axum::http::StatusCode;

#[test]
fn model_not_ready_maps_to_code_and_status() {
    let err = InferenceError::ModelNotReady("not ready".to_string());
    assert_eq!(err.error_code(), "MODEL_NOT_READY");
    assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
}
