//! W3C Trace Context propagation middleware.
//!
//! Extracts `traceparent` and `tracestate` headers from incoming requests,
//! creates or continues traces, and injects headers into outgoing responses.
//!
//! See: https://www.w3.org/TR/trace-context/

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState};
use opentelemetry::Context;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const TRACEPARENT_HEADER: &str = "traceparent";
const TRACESTATE_HEADER: &str = "tracestate";

/// Extension type for storing trace context in request extensions
#[derive(Clone, Debug)]
pub struct TraceContextExtension {
    pub trace_id: String,
    pub span_id: String,
}

/// Extract W3C Trace Context from request headers and propagate to current span.
/// Also injects trace context into response headers for downstream correlation.
pub async fn trace_context_middleware(mut req: Request, next: Next) -> Response {
    // Extract traceparent header
    let traceparent = req
        .headers()
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok());

    let tracestate = req
        .headers()
        .get(TRACESTATE_HEADER)
        .and_then(|v| v.to_str().ok());

    // If traceparent is present, set it as the parent context
    if let Some(traceparent) = traceparent {
        if let Ok(span_context) = parse_traceparent(traceparent, tracestate) {
            let parent_cx = Context::current().with_remote_span_context(span_context.clone());
            // Set the parent context on the current tracing span
            let _ = Span::current().set_parent(parent_cx);

            // Store trace context in request extensions for handlers
            req.extensions_mut().insert(TraceContextExtension {
                trace_id: span_context.trace_id().to_string(),
                span_id: span_context.span_id().to_string(),
            });
        }
    }

    let mut response = next.run(req).await;

    // Inject trace context into response headers for correlation
    let current_span = Span::current();
    let cx = current_span.context();
    let span_ref = cx.span();
    let span_context = span_ref.span_context();

    if span_context.is_valid() {
        let traceparent = format_traceparent(span_context);
        if let Ok(header_value) = HeaderValue::from_str(&traceparent) {
            response
                .headers_mut()
                .insert(TRACEPARENT_HEADER, header_value);
        }

        if let Some(tracestate) = format_tracestate(span_context) {
            if let Ok(header_value) = HeaderValue::from_str(&tracestate) {
                response
                    .headers_mut()
                    .insert(TRACESTATE_HEADER, header_value);
            }
        }
    }

    response
}

/// Parse W3C traceparent header
/// Format: `{version}-{trace-id}-{span-id}-{trace-flags}`
fn parse_traceparent(header: &str, tracestate: Option<&str>) -> Result<SpanContext, &'static str> {
    let parts: Vec<&str> = header.split('-').collect();
    if parts.len() != 4 {
        return Err("Invalid traceparent format");
    }

    if parts[0] != "00" {
        return Err("Unsupported trace version");
    }

    let trace_id = TraceId::from_hex(parts[1]).map_err(|_| "Invalid trace ID")?;
    let span_id = SpanId::from_hex(parts[2]).map_err(|_| "Invalid span ID")?;
    let trace_flags = u8::from_str_radix(parts[3], 16).map_err(|_| "Invalid trace flags")?;

    let trace_state = tracestate
        .and_then(|s| {
            TraceState::from_key_value(s.split(',').filter_map(|kv| {
                let mut parts = kv.splitn(2, '=');
                Some((parts.next()?.trim(), parts.next()?.trim()))
            }))
            .ok()
        })
        .unwrap_or_default();

    Ok(SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::new(trace_flags),
        true, // is_remote
        trace_state,
    ))
}

/// Format SpanContext as W3C traceparent header
fn format_traceparent(span_context: &SpanContext) -> String {
    format!(
        "00-{}-{}-{:02x}",
        span_context.trace_id(),
        span_context.span_id(),
        span_context.trace_flags().to_u8()
    )
}

/// Format tracestate if present
fn format_tracestate(span_context: &SpanContext) -> Option<String> {
    let state = span_context.trace_state();
    if state.header().is_empty() {
        None
    } else {
        Some(state.header())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_traceparent_valid() {
        let header = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let result = parse_traceparent(header, None);
        assert!(result.is_ok());
        let ctx = result.unwrap();
        assert_eq!(
            ctx.trace_id().to_string(),
            "0af7651916cd43dd8448eb211c80319c"
        );
        assert_eq!(ctx.span_id().to_string(), "b7ad6b7169203331");
        assert_eq!(ctx.trace_flags().to_u8(), 1);
    }

    #[test]
    fn test_parse_traceparent_invalid_version() {
        let header = "01-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
        let result = parse_traceparent(header, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_traceparent_invalid_format() {
        let header = "00-invalid";
        let result = parse_traceparent(header, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_traceparent() {
        let trace_id = TraceId::from_hex("0af7651916cd43dd8448eb211c80319c").unwrap();
        let span_id = SpanId::from_hex("b7ad6b7169203331").unwrap();
        let ctx = SpanContext::new(
            trace_id,
            span_id,
            TraceFlags::new(1),
            false,
            TraceState::default(),
        );
        let formatted = format_traceparent(&ctx);
        assert_eq!(
            formatted,
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
        );
    }
}
