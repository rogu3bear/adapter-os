use crate::middleware::context::RequestContext;
use crate::middleware::trace_context::TraceContextExtension;
use crate::request_id::{RequestId, REQUEST_ID_HEADER};
use crate::state::AppState;
use adapteros_api_types::ErrorResponse; // Use standard type
use adapteros_core::{version::GIT_COMMIT_HASH, B3Hash};
use axum::body::to_bytes;
use axum::{
    body::Body,
    extract::Request,
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

/// Wraps each HTTP request with request/response logging and enforces a
/// consistent JSON error envelope on 4xx/5xx responses.
pub async fn observability_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    observability_middleware_inner(Some(state), req, next).await
}

async fn observability_middleware_inner(
    state: Option<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Prefer the request ID already injected by the request_id middleware; fall back
    // to any header-supplied value or generate one if missing.
    let request_id = extract_request_id(&req);
    let (trace_id, span_id) = req
        .extensions()
        .get::<TraceContextExtension>()
        .map(|ctx| (Some(ctx.trace_id.clone()), Some(ctx.span_id.clone())))
        .unwrap_or((None, None));
    let trace_id = trace_id.unwrap_or_default();
    let span_id = span_id.unwrap_or_default();

    let start = Instant::now();
    info!(
        target: "api",
        request_id = %request_id,
        trace_id = trace_id.as_str(),
        span_id = span_id.as_str(),
        component = "adapteros-server-api",
        method = %method,
        path = %path,
        "request_start"
    );

    let mut response = next.run(req).await;
    let status = response.status();
    let duration_ms = start.elapsed().as_millis();

    // Capture context (tenant/user) if present; context_middleware will attach it.
    let ctx = response.extensions().get::<Arc<RequestContext>>().cloned();
    let (tenant_id, user_id, principal_id, principal_type, auth_mode) = ctx
        .as_ref()
        .map(|ctx| {
            (
                Some(ctx.tenant_id().to_string()),
                Some(ctx.user_id().to_string()),
                ctx.principal().map(|p| p.principal_id.to_string()),
                ctx.principal().map(|p| format!("{:?}", p.principal_type)),
                Some(format!("{:?}", ctx.auth_mode())),
            )
        })
        .unwrap_or((None, None, None, None, None));

    ensure_request_id_header(&mut response, &request_id);
    if !trace_id.is_empty() {
        if let Ok(header_value) = HeaderValue::from_str(&trace_id) {
            response.headers_mut().insert("Trace-Id", header_value);
        }
    }

    if status.is_client_error() || status.is_server_error() {
        let (parts, body) = response.into_parts();
        // Cap error body capture to 64KB to avoid unbounded buffering
        let body_bytes = to_bytes(body, 64 * 1024).await.unwrap_or_default();
        let (code, message, hint, detail) = parse_error_payload(&body_bytes, status);
        let hint = hint
            .filter(|hint| !hint.trim().is_empty())
            .unwrap_or_else(|| derive_hint(&code, &message, detail.as_deref(), status));

        let mut envelope = ErrorResponse::new(message.clone())
            .with_code(code.clone())
            .with_request_id(request_id.clone());

        // Session correlation is best-effort; available when clients send X-Session-ID.
        let session_id = ctx.as_ref().and_then(|c| c.session_id.clone());
        envelope.session_id = session_id.clone();

        // Persist 5xx failures as first-class ErrorInstance rows.
        // Also persist worker-originated errors even when non-5xx.
        let worker_originated = is_worker_originated_error(&code, &message, detail.as_deref());
        let should_persist = status.is_server_error() || worker_originated;
        let source = if worker_originated { "worker" } else { "api" };
        let (error_id, fingerprint) = if should_persist {
            let git_sha = if GIT_COMMIT_HASH.is_empty() {
                "unknown"
            } else {
                GIT_COMMIT_HASH
            };

            // Fingerprint recipe (stable, deterministic):
            // error_code + method + path + status + component + git_sha
            let fp_input = format!(
                "code={};method={};path={};status={};component={};git_sha={}",
                code,
                method,
                path,
                status.as_u16(),
                source,
                git_sha
            );
            let fingerprint = B3Hash::hash(fp_input.as_bytes()).to_hex();
            let error_id = adapteros_id::TypedId::new(adapteros_id::IdPrefix::Err).to_string();

            if let Some(state) = state.as_ref() {
                let created_at_unix_ms = state.clock.now_millis() as i64;
                let tenant_id_for_persist = tenant_id.clone().unwrap_or_else(|| "system".into());
                let kind = classify_kind(&code, status);
                let severity = "error";

                let tags_json = serde_json::json!({
                    "http": { "method": method.to_string(), "path": path },
                    "http_status": status.as_u16(),
                    "component": source,
                    "git_sha": git_sha,
                })
                .to_string();

                let row = adapteros_db::errors::ErrorInstanceRow {
                    id: error_id.clone(),
                    created_at_unix_ms,
                    tenant_id: tenant_id_for_persist.clone(),
                    source: source.to_string(),
                    error_code: code.clone(),
                    kind: kind.to_string(),
                    severity: severity.to_string(),
                    message_user: message.clone(),
                    message_dev: detail.clone(),
                    fingerprint: fingerprint.clone(),
                    tags_json,
                    session_id: session_id.clone(),
                    request_id: Some(request_id.clone()),
                    diag_trace_id: None,
                    otel_trace_id: if trace_id.is_empty() {
                        None
                    } else {
                        Some(trace_id.clone())
                    },
                    http_method: Some(method.to_string()),
                    http_path: Some(path.clone()),
                    http_status: Some(status.as_u16() as i32),
                    run_id: None,
                    receipt_hash: None,
                    route_digest: None,
                };

                // Best-effort persistence: never block the response envelope on DB failures.
                match state.db.insert_error_instance(&row).await {
                    Ok(_) => {
                        let _ = state
                            .db
                            .upsert_error_bucket(
                                &tenant_id_for_persist,
                                &fingerprint,
                                &code,
                                &kind,
                                severity,
                                created_at_unix_ms,
                                &error_id,
                            )
                            .await;
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to persist ErrorInstance");
                    }
                }
            }

            (Some(error_id), Some(fingerprint))
        } else {
            (None, None)
        };

        envelope.error_id = error_id;
        envelope.fingerprint = fingerprint;
        envelope.otel_trace_id = if trace_id.is_empty() {
            None
        } else {
            Some(trace_id.clone())
        };

        // Add optional fields
        envelope = envelope.with_hint(hint.clone());

        envelope = if let Some(d) = detail.clone() {
            envelope.with_string_details(d) // Basic string detail if parsed as string, or we need to handle Value
        } else {
            envelope
        };

        // Re-parse detail if it was structured? parse_error_payload returns Option<String>.
        // Ideally we pass serde_json::Value for details.
        // Let's refine parse_error_payload later if needed, for now string details are OK.

        let mut new_response = (status, Json(envelope)).into_response();
        // We generate a fresh, uncompressed error envelope; drop any compression
        // headers from the original response to avoid content-decoding failures
        // in clients when the body no longer matches the encoding.
        new_response.headers_mut().remove(header::CONTENT_ENCODING);

        // Preserve non-content headers from the original response.
        for (name, value) in parts.headers.iter() {
            if name == header::CONTENT_LENGTH
                || name == header::CONTENT_TYPE
                || name == header::CONTENT_ENCODING
            {
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
            trace_id = trace_id.as_str(),
            span_id = span_id.as_str(),
            component = "adapteros-server-api",
            method = %method,
            path = %path,
            status = status.as_u16(),
            code = %code,
            error_message = %message,
            hint = %hint,
            detail = detail.as_deref().unwrap_or(""),
            session_id = session_id.as_deref().unwrap_or(""),
            tenant_id = tenant_id.as_deref().unwrap_or(""),
            user_id = user_id.as_deref().unwrap_or(""),
            principal_id = principal_id.as_deref().unwrap_or(""),
            principal_type = principal_type.as_deref().unwrap_or(""),
            auth_mode = auth_mode.as_deref().unwrap_or(""),
            duration_ms = duration_ms,
            "request_error"
        );

        if let Some(state) = state.as_ref() {
            record_http_metrics(state, status, duration_ms as f64).await;
        }

        new_response
    } else {
        info!(
            target: "api",
            request_id = %request_id,
            trace_id = trace_id.as_str(),
            span_id = span_id.as_str(),
            component = "adapteros-server-api",
            method = %method,
            path = %path,
            status = status.as_u16(),
            duration_ms = duration_ms,
            tenant_id = tenant_id.as_deref().unwrap_or(""),
            user_id = user_id.as_deref().unwrap_or(""),
            principal_id = principal_id.as_deref().unwrap_or(""),
            principal_type = principal_type.as_deref().unwrap_or(""),
            auth_mode = auth_mode.as_deref().unwrap_or(""),
            "request_complete"
        );
        if let Some(state) = state.as_ref() {
            record_http_metrics(state, status, duration_ms as f64).await;
        }
        response
    }
}

fn classify_kind(code: &str, status: StatusCode) -> &'static str {
    let code_upper = code.to_ascii_uppercase();
    if status == StatusCode::GATEWAY_TIMEOUT || code_upper.contains("TIMEOUT") {
        return "timeout";
    }
    if code_upper.contains("UNAUTHORIZED") || code_upper.contains("FORBIDDEN") {
        return "auth";
    }
    if code_upper.contains("VALIDATION") || status == StatusCode::UNPROCESSABLE_ENTITY {
        return "validation";
    }
    if code_upper.contains("NETWORK") || status == StatusCode::BAD_GATEWAY {
        return "network";
    }
    if code_upper.contains("WORKER") {
        return "worker";
    }
    "server"
}

fn is_worker_originated_error(code: &str, message: &str, detail: Option<&str>) -> bool {
    let code_upper = code.to_ascii_uppercase();
    if code_upper.contains("WORKER") || code_upper == "UDS_CONNECTION_FAILED" {
        return true;
    }

    let msg_lower = message.to_ascii_lowercase();
    let detail_lower = detail.unwrap_or("").to_ascii_lowercase();
    let combined = format!("{msg_lower} {detail_lower}");
    combined.contains("worker")
        || combined.contains("uds")
        || combined.contains("unix socket")
        || combined.contains("socket")
}

async fn record_http_metrics(state: &AppState, status: StatusCode, duration_ms: f64) {
    let registry = state.metrics_registry.clone();
    registry
        .record_metric("http_request_duration_ms".to_string(), duration_ms)
        .await;

    if status.is_client_error() || status.is_server_error() {
        let class = status.as_u16() / 100;
        registry
            .record_metric(format!("http_request_error_total.{}xx", class), 1.0)
            .await;
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
        .unwrap_or_else(crate::id_generator::readable_request_id)
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

fn parse_error_payload(
    body: &[u8],
    status: StatusCode,
) -> (String, String, Option<String>, Option<String>) {
    if !body.is_empty() {
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
                let hint = obj
                    .get("hint")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
                return (code, message, hint, detail);
            }
        }

        if let Ok(text) = std::str::from_utf8(body) {
            let message = text.trim();
            if !message.is_empty() {
                return (fallback_code(status), message.to_string(), None, None);
            }
        }
    }

    (fallback_code(status), fallback_message(status), None, None)
}

fn fallback_code(status: StatusCode) -> String {
    format!("HTTP_{}", status.as_u16())
}

fn fallback_message(status: StatusCode) -> String {
    status.canonical_reason().unwrap_or("error").to_string()
}

fn derive_hint(code: &str, message: &str, detail: Option<&str>, _status: StatusCode) -> String {
    let code_upper = code.to_ascii_uppercase();
    let msg_lower = message.to_ascii_lowercase();
    let detail_lower = detail.unwrap_or("").to_ascii_lowercase();
    let combined = format!("{msg_lower} {detail_lower}");

    // --- DB not ready / migrations ---
    if code_upper == "DATABASE_ERROR"
        || code_upper == "DB_ERROR"
        || combined.contains("no such table")
        || combined.contains("migration")
        || combined.contains("schema mismatch")
    {
        return "Database not ready: run `./aosctl db migrate` (or `cargo sqlx migrate run`) and restart the server"
            .to_string();
    }

    if (combined.contains("failed to connect")
        && (combined.contains("database")
            || combined.contains("sqlite")
            || combined.contains("sqlx")))
        || combined.contains("database is locked")
        || combined.contains("sqlite_busy")
        || combined.contains("unable to open database file")
        || combined.contains("readonly database")
    {
        return "Database unavailable: ensure `var/aos-cp.sqlite3` is readable/writable and not locked by another AdapterOS process"
            .to_string();
    }

    // --- No models seeded / model paths ---
    if code_upper == "MODEL_NOT_FOUND"
        || code_upper == "INCOMPATIBLE_BASE_MODEL"
        || combined.contains("model not found")
        || combined.contains("base model")
    {
        return "No base models registered: put a model under `var/model-cache/models` (or `var/models`) and restart with `AOS_SEED_MODEL_CACHE=1`"
            .to_string();
    }

    if (combined.contains("model files") || combined.contains("model path"))
        && (combined.contains("file not found")
            || combined.contains("no such file")
            || combined.contains("path does not exist"))
    {
        return "Model files missing: verify `AOS_MODEL_CACHE_DIR`/`AOS_MODEL_PATH` and restart"
            .to_string();
    }

    // --- Worker not registered / socket issues ---
    if code_upper == "UDS_CONNECTION_FAILED"
        || (combined.contains("uds") && combined.contains("worker"))
        || (combined.contains("socket") && combined.contains("worker"))
    {
        return "Worker socket issue: ensure `AOS_WORKER_SOCKET` matches the worker UDS path (default `/var/run/aos/<tenant>/worker.sock`)"
            .to_string();
    }

    if code_upper == "WORKER_UNAVAILABLE"
        || code_upper == "WORKER_NOT_FOUND"
        || code_upper == "WORKER_NOT_RESPONDING"
        || combined.contains("worker not initialized")
        || combined.contains("no workers registered")
        || combined.contains("no workers available")
        || combined.contains("no worker available")
        || combined.contains("worker not available")
        || combined.contains("no compatible worker")
    {
        return "Worker not registered: start the worker (`./start` with `SKIP_WORKER=0` or `scripts/service-manager.sh start-worker`), then retry"
            .to_string();
    }

    // --- Adapter not found / not active ---
    if code_upper == "ADAPTER_NOT_IN_MANIFEST"
        || code_upper == "ADAPTER_NOT_IN_EFFECTIVE_SET"
        || combined.contains("not in manifest")
        || combined.contains("effective adapter set")
    {
        return "Adapter not active for routing: activate a stack that includes the adapter, then retry"
            .to_string();
    }

    if (code_upper == "NOT_FOUND" && combined.contains("adapter"))
        || combined.contains("adapter not found")
    {
        return "Adapter not found: list adapters (`GET /v1/adapters`) or seed fixtures (`aosctl db seed-fixtures`), then retry with a valid `adapter_id`"
            .to_string();
    }

    // Fallback: always provide something actionable.
    "Check server logs using `request_id` and retry".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        middleware,
        middleware::Next,
        routing::get,
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    async fn observability_middleware_test(req: Request<Body>, next: Next) -> Response {
        super::observability_middleware_inner(None, req, next).await
    }

    #[tokio::test]
    async fn envelopes_errors_and_sets_request_id() {
        let app = Router::new()
            .route(
                "/",
                get(|| async { (StatusCode::BAD_REQUEST, Json(json!({"error": "bad input"}))) }),
            )
            .layer(middleware::from_fn(observability_middleware_test));

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
        let envelope: ErrorResponse = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(envelope.code, "HTTP_400");
        assert_eq!(envelope.message, "bad input");
        assert!(envelope.hint.is_some());
        assert_eq!(envelope.request_id.unwrap(), request_id);
    }

    #[tokio::test]
    async fn passes_success_and_injects_request_id() {
        let app = Router::new()
            .route("/", get(|| async { (StatusCode::OK, "ok") }))
            .layer(middleware::from_fn(observability_middleware_test));

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

    #[tokio::test]
    async fn includes_error_id_and_fingerprint_on_5xx() {
        let app = Router::new()
            .route(
                "/",
                get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
            )
            .layer(middleware::from_fn(observability_middleware_test));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body_bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let envelope: ErrorResponse = serde_json::from_slice(&body_bytes).unwrap();
        assert!(envelope
            .error_id
            .as_deref()
            .unwrap_or("")
            .starts_with("err-"));
        assert!(!envelope.fingerprint.as_deref().unwrap_or("").is_empty());
        assert!(envelope.otel_trace_id.is_some() || envelope.request_id.is_some());
    }

    #[tokio::test]
    async fn includes_error_id_on_worker_originated_non_5xx() {
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "code": "WORKER_UNAVAILABLE",
                            "message": "worker unavailable"
                        })),
                    )
                }),
            )
            .layer(middleware::from_fn(observability_middleware_test));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let envelope: ErrorResponse = serde_json::from_slice(&body_bytes).unwrap();
        assert!(envelope
            .error_id
            .as_deref()
            .unwrap_or("")
            .starts_with("err-"));
        assert!(!envelope.fingerprint.as_deref().unwrap_or("").is_empty());
    }
}
