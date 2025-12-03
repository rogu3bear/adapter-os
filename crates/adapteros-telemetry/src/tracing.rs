//! Distributed tracing correlation for AdapterOS
//!
//! Implements W3C Trace Context for distributed tracing across:
//! - Inference requests
//! - Router decisions
//! - Adapter activations
//! - Policy checks
//! - Database operations
//!
//! Trace IDs propagate through the entire request lifecycle, enabling
//! complete end-to-end visibility and debugging.
//!
//! Implements distributed tracing correlation system

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Trace context following W3C Trace Context specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Unique trace identifier (128-bit)
    pub trace_id: String,
    /// Current span identifier (64-bit)
    pub span_id: String,
    /// Parent span identifier (if this is a child span)
    pub parent_span_id: Option<String>,
    /// Trace flags (sampled, debug, etc.)
    pub trace_flags: u8,
    /// Trace state (vendor-specific data)
    pub trace_state: Option<String>,
}

impl TraceContext {
    /// Create a new root trace context
    pub fn new_root() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            parent_span_id: None,
            trace_flags: 0x01, // Sampled by default
            trace_state: None,
        }
    }

    /// Create a child span from this trace context
    pub fn create_child_span(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            trace_flags: self.trace_flags,
            trace_state: self.trace_state.clone(),
        }
    }

    /// Parse from W3C traceparent header
    ///
    /// Format: `00-{trace-id}-{span-id}-{trace-flags}`
    pub fn from_traceparent(header: &str) -> Result<Self> {
        let parts: Vec<&str> = header.split('-').collect();

        if parts.len() != 4 {
            return Err(AosError::Validation(
                "Invalid traceparent format".to_string(),
            ));
        }

        if parts[0] != "00" {
            return Err(AosError::Validation(
                "Unsupported trace version".to_string(),
            ));
        }

        let trace_id = parts[1].to_string();
        let span_id = parts[2].to_string();
        let trace_flags = u8::from_str_radix(parts[3], 16)
            .map_err(|_| AosError::Validation("Invalid trace flags".to_string()))?;

        Ok(Self {
            trace_id,
            span_id,
            parent_span_id: None,
            trace_flags,
            trace_state: None,
        })
    }

    /// Convert to W3C traceparent header
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id, self.span_id, self.trace_flags
        )
    }

    /// Check if this trace is sampled
    pub fn is_sampled(&self) -> bool {
        self.trace_flags & 0x01 != 0
    }

    /// Mark this trace as sampled
    pub fn set_sampled(&mut self, sampled: bool) {
        if sampled {
            self.trace_flags |= 0x01;
        } else {
            self.trace_flags &= !0x01;
        }
    }
}

/// Span for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Trace context
    pub context: TraceContext,
    /// Span name (operation name)
    pub name: String,
    /// Span kind
    pub kind: SpanKind,
    /// Start time (nanoseconds)
    pub start_time_ns: u64,
    /// End time (nanoseconds, None if ongoing)
    pub end_time_ns: Option<u64>,
    /// Span status
    pub status: SpanStatus,
    /// Span attributes
    pub attributes: HashMap<String, String>,
    /// Events within this span
    pub events: Vec<SpanEvent>,
}

/// Span kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanKind {
    /// Server span (handling a request)
    Server,
    /// Client span (making a request)
    Client,
    /// Internal span (local operation)
    Internal,
    /// Producer span (sending a message)
    Producer,
    /// Consumer span (receiving a message)
    Consumer,
}

/// Span status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpanStatus {
    /// Unset (default)
    Unset,
    /// Ok (success)
    Ok,
    /// Error (failure)
    Error,
}

/// Event within a span
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name
    pub name: String,
    /// Event timestamp (nanoseconds)
    pub timestamp_ns: u64,
    /// Event attributes
    pub attributes: HashMap<String, String>,
}

impl Span {
    /// Create a new span
    pub fn new(context: TraceContext, name: String, kind: SpanKind) -> Self {
        Self {
            context,
            name,
            kind,
            start_time_ns: now_nanos(),
            end_time_ns: None,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Add an attribute to this span
    pub fn set_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }

    /// Add an event to this span
    pub fn add_event(&mut self, name: String, attributes: HashMap<String, String>) {
        self.events.push(SpanEvent {
            name,
            timestamp_ns: now_nanos(),
            attributes,
        });
    }

    /// End this span with a status
    pub fn end(&mut self, status: SpanStatus) {
        self.end_time_ns = Some(now_nanos());
        self.status = status;
    }

    /// Get span duration in nanoseconds (None if ongoing)
    pub fn duration_ns(&self) -> Option<u64> {
        self.end_time_ns
            .map(|end| end.saturating_sub(self.start_time_ns))
    }

    /// Get span duration in milliseconds (None if ongoing)
    pub fn duration_ms(&self) -> Option<f64> {
        self.duration_ns().map(|ns| ns as f64 / 1_000_000.0)
    }
}

/// Trace (collection of related spans)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Trace ID
    pub trace_id: String,
    /// All spans in this trace
    pub spans: Vec<Span>,
    /// Root span ID
    pub root_span_id: String,
}

/// Trace buffer for storing active and completed traces
pub struct TraceBuffer {
    /// Active spans (trace_id -> span_id -> Span)
    active_spans: Arc<RwLock<HashMap<String, HashMap<String, Span>>>>,
    /// Completed traces (trace_id -> Trace)
    completed_traces: Arc<RwLock<HashMap<String, Trace>>>,
    /// Maximum number of completed traces to retain
    max_traces: usize,
}

impl TraceBuffer {
    /// Create a new trace buffer
    pub fn new(max_traces: usize) -> Self {
        Self {
            active_spans: Arc::new(RwLock::new(HashMap::new())),
            completed_traces: Arc::new(RwLock::new(HashMap::new())),
            max_traces,
        }
    }

    /// Start a new span
    pub async fn start_span(&self, span: Span) {
        let mut active = self.active_spans.write().await;

        let trace_id = span.context.trace_id.clone();
        let span_id = span.context.span_id.clone();

        active
            .entry(trace_id)
            .or_insert_with(HashMap::new)
            .insert(span_id, span);
    }

    /// End a span and move to completed if it's the root span
    pub async fn end_span(&self, trace_id: &str, span_id: &str, status: SpanStatus) {
        let mut active = self.active_spans.write().await;

        if let Some(trace_spans) = active.get_mut(trace_id) {
            if let Some(span) = trace_spans.get_mut(span_id) {
                span.end(status);

                // If this was the root span (no parent), move entire trace to completed
                if span.context.parent_span_id.is_none() {
                    // Collect all spans for this trace
                    if let Some(spans_map) = active.remove(trace_id) {
                        let spans: Vec<Span> = spans_map.into_values().collect();

                        if !spans.is_empty() {
                            let trace = Trace {
                                trace_id: trace_id.to_string(),
                                spans,
                                root_span_id: span_id.to_string(),
                            };

                            let mut completed = self.completed_traces.write().await;

                            // Evict oldest trace if at capacity
                            if completed.len() >= self.max_traces {
                                if let Some(oldest_id) = completed.keys().next().cloned() {
                                    completed.remove(&oldest_id);
                                }
                            }

                            completed.insert(trace_id.to_string(), trace);
                        }
                    }
                }
            }
        }
    }

    /// Get an active span
    pub async fn get_active_span(&self, trace_id: &str, span_id: &str) -> Option<Span> {
        let active = self.active_spans.read().await;
        active
            .get(trace_id)
            .and_then(|spans| spans.get(span_id))
            .cloned()
    }

    /// Get a completed trace
    pub async fn get_trace(&self, trace_id: &str) -> Option<Trace> {
        let completed = self.completed_traces.read().await;
        completed.get(trace_id).cloned()
    }

    /// Search traces by criteria
    pub async fn search_traces(&self, query: &TraceSearchQuery) -> Vec<String> {
        let completed = self.completed_traces.read().await;

        completed
            .values()
            .filter(|trace| {
                // Filter by span name if specified
                if let Some(ref name) = query.span_name {
                    if !trace.spans.iter().any(|s| s.name.contains(name)) {
                        return false;
                    }
                }

                // Filter by status if specified
                if let Some(status) = &query.status {
                    if !trace.spans.iter().any(|s| &s.status == status) {
                        return false;
                    }
                }

                // Filter by time range if specified
                if let Some(start_ns) = query.start_time_ns {
                    if !trace.spans.iter().any(|s| s.start_time_ns >= start_ns) {
                        return false;
                    }
                }

                if let Some(end_ns) = query.end_time_ns {
                    if !trace.spans.iter().any(|s| s.start_time_ns <= end_ns) {
                        return false;
                    }
                }

                true
            })
            .map(|trace| trace.trace_id.clone())
            .collect()
    }

    /// Get trace buffer statistics
    pub async fn stats(&self) -> TraceBufferStats {
        let active = self.active_spans.read().await;
        let completed = self.completed_traces.read().await;

        let active_trace_count = active.len();
        let active_span_count: usize = active.values().map(|spans| spans.len()).sum();
        let completed_trace_count = completed.len();

        TraceBufferStats {
            active_trace_count,
            active_span_count,
            completed_trace_count,
            max_traces: self.max_traces,
        }
    }
}

/// Trace search query
#[derive(Debug, Clone)]
pub struct TraceSearchQuery {
    pub span_name: Option<String>,
    pub status: Option<SpanStatus>,
    pub start_time_ns: Option<u64>,
    pub end_time_ns: Option<u64>,
}

/// Trace buffer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceBufferStats {
    pub active_trace_count: usize,
    pub active_span_count: usize,
    pub completed_trace_count: usize,
    pub max_traces: usize,
}

/// Generate a 128-bit trace ID
fn generate_trace_id() -> String {
    let uuid = Uuid::new_v4();
    format!("{:032x}", uuid.as_u128())
}

/// Generate a 64-bit span ID
fn generate_span_id() -> String {
    let uuid = Uuid::new_v4();
    format!("{:016x}", uuid.as_u128() & 0xFFFFFFFFFFFFFFFF)
}

/// Get current time in nanoseconds
fn now_nanos() -> u64 {
    adapteros_core::time::unix_timestamp_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new_root();

        assert_eq!(ctx.trace_id.len(), 32); // 128-bit as hex
        assert_eq!(ctx.span_id.len(), 16); // 64-bit as hex
        assert!(ctx.parent_span_id.is_none());
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_trace_context_child_span() {
        let root = TraceContext::new_root();
        let child = root.create_child_span();

        assert_eq!(child.trace_id, root.trace_id);
        assert_ne!(child.span_id, root.span_id);
        assert_eq!(child.parent_span_id, Some(root.span_id.clone()));
    }

    #[test]
    fn test_traceparent_parsing() {
        let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = TraceContext::from_traceparent(header).unwrap();

        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.span_id, "00f067aa0ba902b7");
        assert_eq!(ctx.trace_flags, 0x01);
        assert!(ctx.is_sampled());
    }

    #[test]
    fn test_traceparent_generation() {
        let ctx = TraceContext {
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            span_id: "00f067aa0ba902b7".to_string(),
            parent_span_id: None,
            trace_flags: 0x01,
            trace_state: None,
        };

        let header = ctx.to_traceparent();
        assert_eq!(
            header,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
    }

    #[tokio::test]
    async fn test_span_lifecycle() {
        let ctx = TraceContext::new_root();
        let mut span = Span::new(ctx, "test_operation".to_string(), SpanKind::Internal);

        assert!(span.end_time_ns.is_none());
        assert_eq!(span.status, SpanStatus::Unset);

        span.set_attribute("test".to_string(), "value".to_string());
        span.add_event("event1".to_string(), HashMap::new());

        span.end(SpanStatus::Ok);

        assert!(span.end_time_ns.is_some());
        assert_eq!(span.status, SpanStatus::Ok);
        assert_eq!(span.attributes.get("test"), Some(&"value".to_string()));
        assert_eq!(span.events.len(), 1);
    }

    #[tokio::test]
    async fn test_trace_buffer() {
        let buffer = TraceBuffer::new(100);

        let ctx = TraceContext::new_root();
        let span = Span::new(ctx.clone(), "root_span".to_string(), SpanKind::Server);

        let trace_id = ctx.trace_id.clone();
        let span_id = ctx.span_id.clone();

        buffer.start_span(span).await;

        let active = buffer.get_active_span(&trace_id, &span_id).await;
        assert!(active.is_some());

        buffer.end_span(&trace_id, &span_id, SpanStatus::Ok).await;

        // Span should now be in completed traces
        let trace = buffer.get_trace(&trace_id).await;
        assert!(trace.is_some());
    }

    #[tokio::test]
    async fn test_trace_search() {
        let buffer = TraceBuffer::new(100);

        let ctx = TraceContext::new_root();
        let span = Span::new(ctx.clone(), "search_test".to_string(), SpanKind::Internal);

        let trace_id = ctx.trace_id.clone();
        let span_id = ctx.span_id.clone();

        buffer.start_span(span).await;
        buffer.end_span(&trace_id, &span_id, SpanStatus::Ok).await;

        let query = TraceSearchQuery {
            span_name: Some("search".to_string()),
            status: None,
            start_time_ns: None,
            end_time_ns: None,
        };

        let results = buffer.search_traces(&query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], trace_id);
    }
}
