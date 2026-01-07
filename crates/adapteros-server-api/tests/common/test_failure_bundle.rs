//! Test Failure Bundle Capture
//!
//! Provides a standard way to capture failure artifacts from E2E, streaming,
//! and replay tests. Artifacts are written to `target/test-failures/` on test failure.
//!
//! # PRD References
//! - PRD-DET-001: Determinism Hardening (failure diagnostics)
//! - PRD-DET-002: Drift Detection (failure evidence)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Global counter for unique failure bundle IDs
static BUNDLE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A captured telemetry event for test diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedTelemetryEvent {
    pub timestamp_ms: u64,
    pub event_type: String,
    pub payload: serde_json::Value,
}

/// Per-stage timing information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageTiming {
    pub stage_name: String,
    pub start_us: u64,
    pub end_us: u64,
    pub duration_us: u64,
}

impl StageTiming {
    pub fn new(name: &str, start: Instant, end: Instant) -> Self {
        let start_us = 0; // Relative to test start
        let duration = end.duration_since(start);
        Self {
            stage_name: name.to_string(),
            start_us,
            end_us: start_us + duration.as_micros() as u64,
            duration_us: duration.as_micros() as u64,
        }
    }
}

/// Failure bundle containing all diagnostic information for a failed test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailureBundle {
    /// Unique trace ID for this test run
    pub trace_id: String,
    /// Test name that failed
    pub test_name: String,
    /// Timestamp when the failure occurred (ISO 8601)
    pub timestamp: String,
    /// Request bytes sent (hex-encoded if binary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bytes: Option<String>,
    /// Response bytes received (hex-encoded if binary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_bytes: Option<String>,
    /// Receipt JSON if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_json: Option<serde_json::Value>,
    /// Captured telemetry events during the test
    pub telemetry_events: Vec<CapturedTelemetryEvent>,
    /// Per-stage timing information
    pub stage_timings: Vec<StageTiming>,
    /// Additional context key-value pairs
    pub context: HashMap<String, String>,
    /// Error message that caused the failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Stack trace if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_trace: Option<String>,
}

impl TestFailureBundle {
    /// Create a new empty failure bundle with a trace ID
    pub fn new(trace_id: &str, test_name: &str) -> Self {
        Self {
            trace_id: trace_id.to_string(),
            test_name: test_name.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_bytes: None,
            response_bytes: None,
            receipt_json: None,
            telemetry_events: Vec::new(),
            stage_timings: Vec::new(),
            context: HashMap::new(),
            error_message: None,
            stack_trace: None,
        }
    }

    /// Generate a unique trace ID for testing
    pub fn generate_trace_id() -> String {
        let counter = BUNDLE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("test-{}-{:04}", timestamp, counter)
    }

    /// Set the request bytes (will be JSON-serialized or hex-encoded)
    pub fn with_request<T: Serialize>(mut self, request: &T) -> Self {
        self.request_bytes = serde_json::to_string_pretty(request).ok();
        self
    }

    /// Set the response bytes (will be JSON-serialized or hex-encoded)
    pub fn with_response<T: Serialize>(mut self, response: &T) -> Self {
        self.response_bytes = serde_json::to_string_pretty(response).ok();
        self
    }

    /// Set the receipt JSON
    pub fn with_receipt<T: Serialize>(mut self, receipt: &T) -> Self {
        self.receipt_json = serde_json::to_value(receipt).ok();
        self
    }

    /// Add a telemetry event
    pub fn add_telemetry_event(&mut self, event_type: &str, payload: serde_json::Value) {
        self.telemetry_events.push(CapturedTelemetryEvent {
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            event_type: event_type.to_string(),
            payload,
        });
    }

    /// Add a stage timing
    pub fn add_stage_timing(&mut self, timing: StageTiming) {
        self.stage_timings.push(timing);
    }

    /// Add context key-value pair
    pub fn add_context(&mut self, key: &str, value: &str) {
        self.context.insert(key.to_string(), value.to_string());
    }

    /// Set the error message
    pub fn with_error(mut self, message: &str) -> Self {
        self.error_message = Some(message.to_string());
        self
    }

    /// Get the output directory for failure bundles
    fn output_dir() -> PathBuf {
        PathBuf::from("target/test-failures")
    }

    /// Save the failure bundle to disk
    ///
    /// Returns the path where the bundle was saved.
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let output_dir = Self::output_dir();
        fs::create_dir_all(&output_dir)?;

        let filename = format!("{}_failure.json", self.trace_id);
        let path = output_dir.join(&filename);

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json)?;

        tracing::info!(
            path = %path.display(),
            trace_id = %self.trace_id,
            "Saved test failure bundle"
        );

        Ok(path)
    }

    /// Save only if there was an error (returns None if no error)
    pub fn save_if_error(&self) -> Option<std::io::Result<PathBuf>> {
        if self.error_message.is_some() {
            Some(self.save())
        } else {
            None
        }
    }
}

/// Test-only telemetry sink that collects events for diagnostics
#[derive(Debug, Default)]
pub struct TestTelemetrySink {
    events: std::sync::Mutex<Vec<CapturedTelemetryEvent>>,
}

impl TestTelemetrySink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a telemetry event
    pub fn record(&self, event_type: &str, payload: serde_json::Value) {
        if let Ok(mut events) = self.events.lock() {
            events.push(CapturedTelemetryEvent {
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                event_type: event_type.to_string(),
                payload,
            });
        }
    }

    /// Get all recorded events
    pub fn drain(&self) -> Vec<CapturedTelemetryEvent> {
        if let Ok(mut events) = self.events.lock() {
            std::mem::take(&mut *events)
        } else {
            Vec::new()
        }
    }

    /// Get count of recorded events
    pub fn len(&self) -> usize {
        self.events.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Timer helper for stage timing capture
pub struct StageTimer {
    name: String,
    start: Instant,
}

impl StageTimer {
    /// Start timing a stage
    pub fn start(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: Instant::now(),
        }
    }

    /// Stop timing and get the StageTiming
    pub fn stop(self) -> StageTiming {
        let end = Instant::now();
        let duration = end.duration_since(self.start);
        StageTiming {
            stage_name: self.name,
            start_us: 0, // Relative timestamps
            end_us: duration.as_micros() as u64,
            duration_us: duration.as_micros() as u64,
        }
    }

    /// Get elapsed time without stopping
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// RAII guard that saves failure bundle on drop if there was an error
pub struct FailureBundleGuard {
    bundle: TestFailureBundle,
    should_save: bool,
}

impl FailureBundleGuard {
    pub fn new(trace_id: &str, test_name: &str) -> Self {
        Self {
            bundle: TestFailureBundle::new(trace_id, test_name),
            should_save: false,
        }
    }

    /// Mark that an error occurred and bundle should be saved
    pub fn mark_failed(&mut self, error: &str) {
        self.bundle.error_message = Some(error.to_string());
        self.should_save = true;
    }

    /// Get mutable access to the bundle
    pub fn bundle_mut(&mut self) -> &mut TestFailureBundle {
        &mut self.bundle
    }

    /// Get immutable access to the bundle
    pub fn bundle(&self) -> &TestFailureBundle {
        &self.bundle
    }

    /// Force save regardless of error status
    pub fn force_save(&mut self) {
        self.should_save = true;
    }

    /// Cancel saving (e.g., on test success)
    pub fn cancel_save(&mut self) {
        self.should_save = false;
    }
}

impl Drop for FailureBundleGuard {
    fn drop(&mut self) {
        if self.should_save {
            if let Err(e) = self.bundle.save() {
                eprintln!("Failed to save failure bundle: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failure_bundle_creation() {
        let trace_id = TestFailureBundle::generate_trace_id();
        let bundle = TestFailureBundle::new(&trace_id, "test_example");

        assert_eq!(bundle.trace_id, trace_id);
        assert_eq!(bundle.test_name, "test_example");
        assert!(bundle.telemetry_events.is_empty());
    }

    #[test]
    fn test_failure_bundle_with_context() {
        let bundle = TestFailureBundle::new("trace-123", "test_example")
            .with_error("Something went wrong");

        assert_eq!(bundle.error_message.as_deref(), Some("Something went wrong"));
    }

    #[test]
    fn test_stage_timer() {
        let timer = StageTimer::start("test_stage");
        std::thread::sleep(std::time::Duration::from_millis(10));
        let timing = timer.stop();

        assert_eq!(timing.stage_name, "test_stage");
        assert!(timing.duration_us >= 10_000); // At least 10ms in microseconds
    }

    #[test]
    fn test_telemetry_sink() {
        let sink = TestTelemetrySink::new();

        sink.record("test_event", serde_json::json!({"key": "value"}));
        assert_eq!(sink.len(), 1);

        let events = sink.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "test_event");
        assert!(sink.is_empty());
    }
}
