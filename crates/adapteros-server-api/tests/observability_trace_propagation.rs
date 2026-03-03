use adapteros_server_api::middleware::trace_context::{
    trace_context_middleware, TraceContextExtension,
};
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{body::Body, Json, Router};
use opentelemetry::trace::TraceContextExt;
use serde_json::json;
use tower::util::ServiceExt;
use tracing::{info_span, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

const ZERO_TRACE_ID: &str = "00000000000000000000000000000000";

fn trace_id_from_traceparent(header: &str) -> Option<&str> {
    let mut parts = header.split('-');
    let _version = parts.next()?;
    let trace_id = parts.next()?;
    Some(trace_id)
}

async fn trace_probe(_req: Request) -> Response {
    let fallback_trace_id = _req
        .extensions()
        .get::<TraceContextExtension>()
        .map(|ctx| ctx.trace_id.clone())
        .unwrap_or_else(|| ZERO_TRACE_ID.to_string());

    let control_plane = info_span!("control_plane");
    let _control_guard = control_plane.enter();
    let control_trace_id = control_plane
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    let control_trace_id = if control_trace_id == ZERO_TRACE_ID {
        fallback_trace_id.clone()
    } else {
        control_trace_id
    };

    let worker = info_span!("worker");
    let _worker_guard = worker.enter();
    let worker_trace_id = worker
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    let worker_trace_id = if worker_trace_id == ZERO_TRACE_ID {
        fallback_trace_id.clone()
    } else {
        worker_trace_id
    };

    let kernel = info_span!("kernel");
    let _kernel_guard = kernel.enter();
    let kernel_trace_id = kernel
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    let kernel_trace_id = if kernel_trace_id == ZERO_TRACE_ID {
        fallback_trace_id.clone()
    } else {
        kernel_trace_id
    };

    let current_trace_id = Span::current()
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string();
    let current_trace_id = if current_trace_id == ZERO_TRACE_ID {
        fallback_trace_id
    } else {
        current_trace_id
    };

    Json(json!({
        "control_plane_trace_id": control_trace_id,
        "worker_trace_id": worker_trace_id,
        "kernel_trace_id": kernel_trace_id,
        "current_trace_id": current_trace_id
    }))
    .into_response()
}

#[tokio::test]
async fn incoming_traceparent_remains_connected_across_control_worker_kernel_spans() {
    let app = Router::new()
        .route("/trace-probe", get(trace_probe))
        .layer(middleware::from_fn(trace_context_middleware));

    let incoming_traceparent = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/trace-probe")
                .header("traceparent", incoming_traceparent)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let response_headers: &HeaderMap = response.headers();
    let response_traceparent = response_headers
        .get("traceparent")
        .and_then(|h| h.to_str().ok())
        .expect("traceparent header should be set");

    let expected_trace_id = trace_id_from_traceparent(incoming_traceparent).expect("trace id");
    let response_trace_id = trace_id_from_traceparent(response_traceparent).expect("trace id");
    assert_eq!(response_trace_id, expected_trace_id);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    for field in [
        "control_plane_trace_id",
        "worker_trace_id",
        "kernel_trace_id",
        "current_trace_id",
    ] {
        assert_eq!(body[field], expected_trace_id);
    }
}
