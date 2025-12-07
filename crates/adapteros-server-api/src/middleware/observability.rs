use crate::middleware::context::RequestContext;
use crate::request_id::{RequestId, REQUEST_ID_HEADER};
use crate::types::ApiErrorBody;
use axum::body::to_bytes;
use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};
use uuid::Uuid;

/// Wraps each HTTP request with request/response logging and enforces a
/// consistent JSON error envelope on 4xx/5xx responses.
pub async fn observability_middleware(req: Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Prefer the request ID already injected by the request_id middleware; fall back
    // to any header-supplied value or generate one if missing.
    let request_id = extract_request_id(&req);

    let start = Instant::now();
    info!(
        target: "api",
        request_id = %request_id,
        method = %method,
        path = %path,
        "request_start"
    );

    let mut response = next.run(req).await;
    let status = response.status();
    let duration_ms = start.elapsed().as_millis();

    // Capture context (tenant/user) if present; context_middleware will attach it.
    let ctx = response.extensions().get::<Arc<RequestContext>>().cloned();
    let (tenant_id, user_id) = ctx
        .as_ref()
        .map(|ctx| {
            (
                Some(ctx.tenant_id().to_string()),
                Some(ctx.user_id().to_string()),
            )
        })
        .unwrap_or((None, None));

    ensure_request_id_header(&mut response, &request_id);

    if status.is_client_error() || status.is_server_error() {
        let (parts, body) = response.into_parts();
        // Cap error body capture to 64KB to avoid unbounded buffering
        let body_bytes = to_bytes(body, 64 * 1024).await.unwrap_or_default();
        let (code, message, detail) = parse_error_payload(&body_bytes, status);

        let envelope = ApiErrorBody {
            code: code.clone(),
            message: message.clone(),
            detail: detail.clone(),
            request_id: request_id.clone(),
        };

        let mut new_response = (status, Json(envelope)).into_response();

        // Preserve non-content headers from the original response.
        for (name, value) in parts.headers.iter() {
            if name == header::CONTENT_LENGTH || name == header::CONTENT_TYPE {
                continue;
            }
            new_response
                .headers_mut()
                .insert(name.clone(), value.clone());
        }

        ensure_request_id_header(&mut new_response, &request_id);

        error!(
            target: "api",
            request_id = %request_id,
            method = %method,
            path = %path,
            status = status.as_u16(),
            code = %code,
            error_message = %message,
            detail = detail.as_deref().unwrap_or(""),
            tenant_id = tenant_id.as_deref().unwrap_or(""),
            user_id = user_id.as_deref().unwrap_or(""),
            duration_ms = duration_ms,
            "request_error"
        );

        new_response
    } else {
        info!(
            target: "api",
            request_id = %request_id,
            method = %method,
            path = %path,
            status = status.as_u16(),
            duration_ms = duration_ms,
            tenant_id = tenant_id.as_deref().unwrap_or(""),
            user_id = user_id.as_deref().unwrap_or(""),
            "request_complete"
        );
        response
    }
}

fn extract_request_id(req: &Request<Body>) -> String {
    req.extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .or_else(|| {
            req.headers()
                .get(REQUEST_ID_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn ensure_request_id_header(response: &mut Response, request_id: &str) {
    if response.headers().get(REQUEST_ID_HEADER).is_none() {
        if let Ok(header_value) = HeaderValue::from_str(request_id) {
            response
                .headers_mut()
                .insert(REQUEST_ID_HEADER, header_value);
        }
    }
}

#[derive(Deserialize)]
struct EnvelopeLike {
    code: Option<String>,
    message: Option<String>,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct LegacyErrorResponse {
    error: Option<String>,
    code: Option<String>,
    details: Option<serde_json::Value>,
}

fn parse_error_payload(body: &[u8], status: StatusCode) -> (String, String, Option<String>) {
    if !body.is_empty() {
        if let Ok(envelope) = serde_json::from_slice::<EnvelopeLike>(body) {
            if envelope.code.is_some() || envelope.message.is_some() {
                let code = envelope.code.unwrap_or_else(|| fallback_code(status));
                let message = envelope.message.unwrap_or_else(|| fallback_message(status));
                return (code, message, envelope.detail);
            }
        }

        if let Ok(legacy) = serde_json::from_slice::<LegacyErrorResponse>(body) {
            let code = legacy.code.unwrap_or_else(|| fallback_code(status));
            let message = legacy.error.unwrap_or_else(|| fallback_message(status));
            let detail = legacy.details.and_then(|d| serde_json::to_string(&d).ok());
            return (code, message, detail);
        }

        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
            if let Some(obj) = value.as_object() {
                let code = obj
                    .get("code")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fallback_code(status));
                let message = obj
                    .get("message")
                    .or_else(|| obj.get("error"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| fallback_message(status));
                let detail = obj
                    .get("detail")
                    .or_else(|| obj.get("details"))
                    .and_then(|v| {
                        if v.is_string() {
                            v.as_str().map(|s| s.to_string())
                        } else {
                            serde_json::to_string(v).ok()
                        }
                    });
                return (code, message, detail);
            }
        }
    }

    (fallback_code(status), fallback_message(status), None)
}

fn fallback_code(status: StatusCode) -> String {
    format!("HTTP_{}", status.as_u16())
}

fn fallback_message(status: StatusCode) -> String {
    status.canonical_reason().unwrap_or("error").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn envelopes_errors_and_sets_request_id() {
        let app = Router::new()
            .route(
                "/",
                get(|| async { (StatusCode::BAD_REQUEST, Json(json!({"error": "bad input"}))) }),
            )
            .layer(middleware::from_fn(observability_middleware));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let request_id = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .expect("request id header set");

        let body_bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let envelope: ApiErrorBody = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(envelope.code, "HTTP_400");
        assert_eq!(envelope.message, "bad input");
        assert_eq!(envelope.request_id, request_id);
    }

    #[tokio::test]
    async fn passes_success_and_injects_request_id() {
        let app = Router::new()
            .route("/", get(|| async { (StatusCode::OK, "ok") }))
            .layer(middleware::from_fn(observability_middleware));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response.headers().get(REQUEST_ID_HEADER);
        assert!(request_id.is_some());

        let body_bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        assert_eq!(body_bytes.as_ref(), b"ok");
    }
}
