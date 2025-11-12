use adapteros_core::AosError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::warn;

pub fn db_error_to_response(e: AosError) -> Response {
    warn!(error = %e, "DB error response");
    let body = json!({ "error": e.to_string() });
    (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
}

pub fn validation_error(msg: &str) -> Response {
    let body = json!({ "error": "Validation failed", "detail": msg });
    (StatusCode::BAD_REQUEST, body).into_response()
}
