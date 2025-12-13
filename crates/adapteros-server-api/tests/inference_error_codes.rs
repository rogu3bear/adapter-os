use adapteros_api_types::FailureCode;
use adapteros_server_api::types::InferenceError;
use axum::http::StatusCode;
use axum::Json;

#[test]
fn model_not_ready_maps_to_code_and_status() {
    let err = InferenceError::ModelNotReady("not ready".to_string());
    assert_eq!(err.error_code(), "MODEL_NOT_READY");
    assert_eq!(err.status_code(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn worker_oom_maps_to_failure_code() {
    let err = InferenceError::WorkerError("out of memory in worker".to_string());
    let failure = err.failure_code();
    assert_eq!(failure, Some(FailureCode::OutOfMemory));

    let (_status, Json(body)) =
        <(StatusCode, Json<adapteros_server_api::types::ErrorResponse>)>::from(err);
    assert_eq!(body.failure_code, Some(FailureCode::OutOfMemory));
}

#[test]
fn tenant_denied_maps_to_failure_code() {
    let err = InferenceError::PermissionDenied("tenant denied".to_string());
    assert_eq!(err.failure_code(), Some(FailureCode::TenantAccessDenied));

    let (_status, Json(body)) =
        <(StatusCode, Json<adapteros_server_api::types::ErrorResponse>)>::from(err);
    assert_eq!(body.failure_code, Some(FailureCode::TenantAccessDenied));
}
