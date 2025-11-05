//! In-memory trace builder with bounded buffers for live dashboard

use crate::schema::TraceBundle;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// A trace span with parent/child relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: String,
    pub trace_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub start_ns: u64,
    pub end_ns: Option<u64>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub events: Vec<SpanEvent>,
    pub status: SpanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatus {
    Unset,
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp_ns: u64,
    pub attributes: HashMap<String, serde_json::Value>,
}

/// A complete trace (tree of spans)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: String,
    pub spans: Vec<Span>,
    pub root_span_id: Option<String>,
}

/// Bounded in-memory store for traces
#[derive(Debug, Clone)]
pub struct TraceBuffer {
    capacity: usize,
    inner: Arc<RwLock<VecDeque<Trace>>>,
    span_index: Arc<RwLock<HashMap<String, Vec<String>>>>, // trace_id -> span_ids
}

impl TraceBuffer {
    /// Create a new trace buffer with the given maximum number of traces
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            span_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add or update a trace (evicts oldest if over capacity)
    pub fn add_trace(&self, trace: Trace) {
        let mut guard = self.inner.write().expect("trace buffer poisoned");
        let mut idx_guard = self.span_index.write().expect("span index poisoned");

        // Evict oldest if at capacity
        if guard.len() >= self.capacity {
            if let Some(old_trace) = guard.pop_front() {
                idx_guard.remove(&old_trace.trace_id);
            }
        }

        // Index span IDs by trace ID
        let span_ids: Vec<String> = trace.spans.iter().map(|s| s.span_id.clone()).collect();
        idx_guard.insert(trace.trace_id.clone(), span_ids);

        guard.push_back(trace);
    }

    /// Get a trace by trace_id
    pub fn get_trace(&self, trace_id: &str) -> Option<Trace> {
        let guard = self.inner.read().expect("trace buffer poisoned");
        guard.iter().find(|t| t.trace_id == trace_id).cloned()
    }

    /// Search traces by various criteria (returns matching trace_ids)
    pub fn search(&self, query: &TraceSearchQuery) -> Vec<String> {
        let guard = self.inner.read().expect("trace buffer poisoned");
        let mut results = Vec::new();

        for trace in guard.iter() {
            let mut matches = true;

            if let Some(ref span_name) = query.span_name {
                if !trace.spans.iter().any(|s| &s.name == span_name) {
                    matches = false;
                }
            }

            if let Some(ref _status) = query.status {
                if !trace.spans.iter().any(|s| matches!(&s.status, _status)) {
                    matches = false;
                }
            }

            if let Some(start_ns) = query.start_time_ns {
                // Check if any span starts after this time
                if !trace.spans.iter().any(|s| s.start_ns >= start_ns) {
                    matches = false;
                }
            }

            if let Some(end_ns) = query.end_time_ns {
                // Check if any span ends before this time
                if !trace
                    .spans
                    .iter()
                    .any(|s| s.end_ns.unwrap_or(u64::MAX) <= end_ns)
                {
                    matches = false;
                }
            }

            if matches {
                results.push(trace.trace_id.clone());
            }
        }

        results
    }

    /// Get all trace IDs (newest first, up to limit)
    pub fn list_traces(&self, limit: usize) -> Vec<String> {
        let guard = self.inner.read().expect("trace buffer poisoned");
        guard
            .iter()
            .rev()
            .take(limit)
            .map(|t| t.trace_id.clone())
            .collect()
    }

    /// Current number of traces retained
    pub fn len(&self) -> usize {
        let guard = self.inner.read().expect("trace buffer poisoned");
        guard.len()
    }

    /// Check if the trace buffer is empty
    pub fn is_empty(&self) -> bool {
        let guard = self.inner.read().expect("trace buffer poisoned");
        guard.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct TraceSearchQuery {
    pub span_name: Option<String>,
    pub status: Option<SpanStatus>,
    pub start_time_ns: Option<u64>,
    pub end_time_ns: Option<u64>,
}

/// Trace builder for constructing spans and traces from events
pub struct TraceBuilder {
    trace_id: String,
    spans: HashMap<String, Span>,
    root_span_id: Option<String>,
}

impl TraceBuilder {
    /// Create a new trace builder with a generated trace ID
    pub fn new() -> Self {
        Self::with_trace_id(Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string())
    }

    /// Create a new trace builder with a specific trace ID
    pub fn with_trace_id(trace_id: String) -> Self {
        Self {
            trace_id,
            spans: HashMap::new(),
            root_span_id: None,
        }
    }

    /// Start a new span
    pub fn start_span(
        &mut self,
        span_id: String,
        name: String,
        parent_id: Option<String>,
        start_ns: u64,
    ) {
        if self.root_span_id.is_none() && parent_id.is_none() {
            self.root_span_id = Some(span_id.clone());
        }

        let span = Span {
            span_id,
            trace_id: self.trace_id.clone(),
            parent_id,
            name,
            start_ns,
            end_ns: None,
            attributes: HashMap::new(),
            events: Vec::new(),
            status: SpanStatus::Unset,
        };

        self.spans.insert(span.span_id.clone(), span);
    }

    /// End a span
    pub fn end_span(&mut self, span_id: &str, end_ns: u64, status: SpanStatus) {
        if let Some(span) = self.spans.get_mut(span_id) {
            span.end_ns = Some(end_ns);
            span.status = status;
        }
    }

    /// Add an attribute to a span
    pub fn add_attribute(&mut self, span_id: &str, key: String, value: serde_json::Value) {
        if let Some(span) = self.spans.get_mut(span_id) {
            span.attributes.insert(key, value);
        }
    }

    /// Add an event to a span
    pub fn add_span_event(&mut self, span_id: &str, event: SpanEvent) {
        if let Some(span) = self.spans.get_mut(span_id) {
            span.events.push(event);
        }
    }

    /// Build the final trace
    pub fn build(self) -> Trace {
        Trace {
            trace_id: self.trace_id,
            spans: self.spans.into_values().collect(),
            root_span_id: self.root_span_id,
        }
    }
}

impl Default for TraceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a TraceBundle (from schema) into a Trace (for dashboard)
pub fn bundle_to_trace(bundle: &TraceBundle) -> Trace {
    let mut spans = Vec::new();
    let mut root_span_id: Option<String> = None;

    for (idx, event) in bundle.events.iter().enumerate() {
        let span_id = format!("span_{}", idx);

        if idx == 0 {
            root_span_id = Some(span_id.clone());
        }

        let start_ns = event.logical_timestamp.global_tick * 1000; // Approximate ns
        let end_ns = if idx < bundle.events.len() - 1 {
            Some((event.logical_timestamp.global_tick + 1) * 1000)
        } else {
            None
        };

        let mut attributes = HashMap::new();
        attributes.insert(
            "op_id".to_string(),
            serde_json::Value::String(event.op_id.clone()),
        );
        attributes.insert(
            "event_type".to_string(),
            serde_json::Value::String(event.event_type.clone()),
        );

        let span = Span {
            span_id,
            trace_id: bundle.bundle_id.to_string(),
            parent_id: if idx > 0 {
                Some(format!("span_{}", idx - 1))
            } else {
                None
            },
            name: event.event_type.clone(),
            start_ns,
            end_ns,
            attributes,
            events: Vec::new(),
            status: SpanStatus::Ok,
        };

        spans.push(span);
    }

    Trace {
        trace_id: bundle.bundle_id.to_string(),
        spans,
        root_span_id,
    }
}
