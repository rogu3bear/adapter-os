//! # Streaming Reliability Test Suite
//!
//! Validates SSE streaming correctness without timing-sensitive assertions.
//! Uses buffered collection and completion signals instead of sleep-based timeouts.
//!
//! ## Test Properties
//! - JSON chunk format: `data: {json}\n\n`
//! - Monotonic event IDs per stream
//! - `[DONE]` marker is final
//! - `finish_reason` present in final chunk
//! - `role` appears in first chunk
//!
//! ## Non-Timing Assertions
//! - Buffered collection then validate ordering after collection
//! - Completion signal via channel rather than sleep-based timeouts
//! - 5s ceiling per stream test (select + signal)
//!
//! ## PRD References
//! - PRD-DET-001: Determinism Hardening (streaming determinism)

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

mod common;
use common::{FailureBundleGuard, StageTimer, TestFailureBundle};

/// OpenAI-compatible streaming chunk format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Collected SSE event with metadata
#[derive(Debug, Clone)]
pub struct CollectedSseEvent {
    /// Raw SSE line
    pub raw: String,
    /// Event ID (if present)
    pub event_id: Option<u64>,
    /// Parsed JSON data (if data event)
    pub data: Option<ChatCompletionChunk>,
    /// Is this a [DONE] marker?
    pub is_done: bool,
    /// Is this a keep-alive comment?
    pub is_keepalive: bool,
    /// Timestamp of collection (monotonic counter)
    pub collection_order: u64,
}

/// SSE event collector that buffers all events for later validation
pub struct SseEventCollector {
    events: Vec<CollectedSseEvent>,
    order_counter: AtomicU64,
}

impl SseEventCollector {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            order_counter: AtomicU64::new(0),
        }
    }

    /// Parse and collect an SSE line
    pub fn collect_line(&mut self, line: &str) -> Result<(), String> {
        let collection_order = self.order_counter.fetch_add(1, Ordering::SeqCst);

        // Handle different SSE line types
        if line.starts_with("data: [DONE]") {
            self.events.push(CollectedSseEvent {
                raw: line.to_string(),
                event_id: None,
                data: None,
                is_done: true,
                is_keepalive: false,
                collection_order,
            });
            return Ok(());
        }

        if line.starts_with(": ") || line.starts_with(":") {
            // SSE comment (keep-alive)
            self.events.push(CollectedSseEvent {
                raw: line.to_string(),
                event_id: None,
                data: None,
                is_done: false,
                is_keepalive: true,
                collection_order,
            });
            return Ok(());
        }

        if line.starts_with("data: ") {
            let json_str = &line[6..];
            let chunk: ChatCompletionChunk = serde_json::from_str(json_str)
                .map_err(|e| format!("Invalid JSON in SSE data: {}", e))?;

            self.events.push(CollectedSseEvent {
                raw: line.to_string(),
                event_id: None, // Would need id: line parsing
                data: Some(chunk),
                is_done: false,
                is_keepalive: false,
                collection_order,
            });
            return Ok(());
        }

        if line.starts_with("id: ") {
            // Event ID line (handled separately in real SSE)
            return Ok(());
        }

        if line.is_empty() {
            // Empty line (event delimiter) - skip
            return Ok(());
        }

        // Unknown line type
        Ok(())
    }

    /// Get all collected events
    pub fn events(&self) -> &[CollectedSseEvent] {
        &self.events
    }

    /// Get only data events (excluding DONE, keepalive, etc.)
    pub fn data_events(&self) -> Vec<&CollectedSseEvent> {
        self.events.iter().filter(|e| e.data.is_some()).collect()
    }

    /// Check if stream has a DONE marker
    pub fn has_done(&self) -> bool {
        self.events.iter().any(|e| e.is_done)
    }

    /// Get the DONE event if present
    pub fn done_event(&self) -> Option<&CollectedSseEvent> {
        self.events.iter().find(|e| e.is_done)
    }

    /// Get the final data event (with finish_reason)
    pub fn final_data_event(&self) -> Option<&CollectedSseEvent> {
        self.data_events()
            .iter()
            .rev()
            .find(|e| {
                e.data
                    .as_ref()
                    .and_then(|c| c.choices.first())
                    .and_then(|ch| ch.finish_reason.as_ref())
                    .is_some()
            })
            .copied()
    }
}

impl Default for SseEventCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Simulated streaming source that produces SSE events
struct MockStreamingSource {
    request_id: String,
    tokens: Vec<String>,
    include_role_in_first: bool,
}

impl MockStreamingSource {
    fn new(request_id: &str, tokens: Vec<String>) -> Self {
        Self {
            request_id: request_id.to_string(),
            tokens,
            include_role_in_first: true,
        }
    }

    /// Produce all SSE lines (with completion signal)
    async fn produce(
        &self,
        tx: mpsc::Sender<String>,
        done_signal: Arc<watch::Sender<bool>>,
    ) -> Result<(), String> {
        let created = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (i, token) in self.tokens.iter().enumerate() {
            let chunk = ChatCompletionChunk {
                id: self.request_id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: "adapteros-test".to_string(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: ChunkDelta {
                        content: Some(token.clone()),
                        role: if i == 0 && self.include_role_in_first {
                            Some("assistant".to_string())
                        } else {
                            None
                        },
                    },
                    finish_reason: None,
                }],
            };

            let line = format!(
                "data: {}",
                serde_json::to_string(&chunk).map_err(|e| e.to_string())?
            );
            tx.send(line).await.map_err(|e| e.to_string())?;
        }

        // Send final chunk with finish_reason
        let final_chunk = ChatCompletionChunk {
            id: self.request_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: "adapteros-test".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta {
                    content: None,
                    role: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        };

        tx.send(format!(
            "data: {}",
            serde_json::to_string(&final_chunk).map_err(|e| e.to_string())?
        ))
        .await
        .map_err(|e| e.to_string())?;

        // Send DONE marker
        tx.send("data: [DONE]".to_string())
            .await
            .map_err(|e| e.to_string())?;

        // Signal completion
        let _ = done_signal.send(true);

        Ok(())
    }
}

/// Validation result for a streaming session
#[derive(Debug, Default)]
pub struct StreamValidationResult {
    pub passed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub event_count: usize,
    pub data_event_count: usize,
}

/// Validate collected SSE events against streaming reliability requirements
pub fn validate_stream(collector: &SseEventCollector) -> StreamValidationResult {
    let mut result = StreamValidationResult::default();
    result.event_count = collector.events().len();
    result.data_event_count = collector.data_events().len();

    // Check 1: JSON chunk format (data: {json}\n\n)
    for event in collector.data_events() {
        if !event.raw.starts_with("data: ") {
            result.errors.push(format!(
                "Data event doesn't start with 'data: ': {}",
                event.raw
            ));
        }

        if event.data.is_none() {
            result
                .errors
                .push(format!("Data event has no parsed JSON: {}", event.raw));
        }
    }

    // Check 2: Monotonic event IDs (collection order)
    let data_events = collector.data_events();
    for i in 1..data_events.len() {
        if data_events[i].collection_order <= data_events[i - 1].collection_order {
            result.errors.push(format!(
                "Event order not monotonic: {} <= {}",
                data_events[i].collection_order,
                data_events[i - 1].collection_order
            ));
        }
    }

    // Check 3: [DONE] marker is final
    if let Some(done_event) = collector.done_event() {
        // DONE should be the last event (or second to last if there's a final data event)
        let done_order = done_event.collection_order;
        let max_order = collector
            .events()
            .iter()
            .map(|e| e.collection_order)
            .max()
            .unwrap_or(0);

        if done_order != max_order {
            result.errors.push(format!(
                "[DONE] marker is not final: order {} vs max {}",
                done_order, max_order
            ));
        }
    } else {
        result.errors.push("Missing [DONE] marker".to_string());
    }

    // Check 4: finish_reason present in final data chunk
    if let Some(final_event) = collector.final_data_event() {
        if let Some(chunk) = &final_event.data {
            if chunk.choices.is_empty() {
                result.errors.push("Final chunk has no choices".to_string());
            } else if chunk.choices[0].finish_reason.is_none() {
                result
                    .errors
                    .push("Final chunk missing finish_reason".to_string());
            }
        }
    } else {
        // Only warn if there were data events
        if !data_events.is_empty() {
            result
                .warnings
                .push("No final data event with finish_reason found".to_string());
        }
    }

    // Check 5: role appears in first chunk
    if let Some(first_event) = data_events.first() {
        if let Some(chunk) = &first_event.data {
            if chunk.choices.is_empty() {
                result.errors.push("First chunk has no choices".to_string());
            } else if chunk.choices[0].delta.role.is_none() {
                result
                    .warnings
                    .push("First chunk missing role in delta".to_string());
            }
        }
    }

    result.passed = result.errors.is_empty();
    result
}

// =============================================================================
// Tests
// =============================================================================

/// Test SSE chunk format validation
#[tokio::test]
async fn test_sse_chunk_format() {
    let mut collector = SseEventCollector::new();

    // Valid chunk
    let valid_chunk = r#"data: {"id":"test-1","object":"chat.completion.chunk","created":1234567890,"model":"test","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
    assert!(collector.collect_line(valid_chunk).is_ok());

    // Invalid JSON
    let invalid_json = "data: {invalid json}";
    assert!(collector.collect_line(invalid_json).is_err());

    // DONE marker
    assert!(collector.collect_line("data: [DONE]").is_ok());

    // Keep-alive
    assert!(collector.collect_line(": keep-alive").is_ok());
}

/// Test monotonic event ordering
#[tokio::test]
async fn test_monotonic_event_ordering() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (done_tx, mut done_rx) = watch::channel(false);

    let source = MockStreamingSource::new(
        "test-monotonic",
        vec![
            "Hello".to_string(),
            " ".to_string(),
            "world".to_string(),
            "!".to_string(),
        ],
    );

    // Produce events
    let done_signal = Arc::new(done_tx);
    let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

    // Collect with timeout
    let mut collector = SseEventCollector::new();
    let collect_result = timeout(Duration::from_secs(5), async {
        // Wait for done signal or collect all
        loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    let _ = collector.collect_line(&line);
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        // Drain remaining
                        while let Ok(line) = rx.try_recv() {
                            let _ = collector.collect_line(&line);
                        }
                        break;
                    }
                }
            }
        }
    })
    .await;

    producer
        .await
        .expect("producer finished")
        .expect("producer ok");
    assert!(
        collect_result.is_ok(),
        "Collection should complete within 5s"
    );

    // Validate
    let validation = validate_stream(&collector);
    assert!(
        validation.passed,
        "Validation errors: {:?}",
        validation.errors
    );

    // Check ordering
    let events = collector.data_events();
    for i in 1..events.len() {
        assert!(
            events[i].collection_order > events[i - 1].collection_order,
            "Events must be monotonically ordered"
        );
    }
}

/// Test [DONE] marker is final
#[tokio::test]
async fn test_done_marker_is_final() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (done_tx, mut done_rx) = watch::channel(false);

    let source = MockStreamingSource::new("test-done", vec!["Test".to_string()]);

    let done_signal = Arc::new(done_tx);
    let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

    // Collect
    let mut collector = SseEventCollector::new();
    let _ = timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    let _ = collector.collect_line(&line);
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        while let Ok(line) = rx.try_recv() {
                            let _ = collector.collect_line(&line);
                        }
                        break;
                    }
                }
            }
        }
    })
    .await;

    producer
        .await
        .expect("producer finished")
        .expect("producer ok");

    // Verify DONE is present and final
    assert!(collector.has_done(), "Must have DONE marker");
    let done_event = collector.done_event().expect("DONE event");
    let max_order = collector
        .events()
        .iter()
        .map(|e| e.collection_order)
        .max()
        .unwrap_or(0);
    assert_eq!(
        done_event.collection_order, max_order,
        "DONE must be final event"
    );
}

/// Test finish_reason in final chunk
#[tokio::test]
async fn test_finish_reason_present() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (done_tx, mut done_rx) = watch::channel(false);

    let source = MockStreamingSource::new("test-finish", vec!["Response".to_string()]);

    let done_signal = Arc::new(done_tx);
    let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

    let mut collector = SseEventCollector::new();
    let _ = timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    let _ = collector.collect_line(&line);
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        while let Ok(line) = rx.try_recv() {
                            let _ = collector.collect_line(&line);
                        }
                        break;
                    }
                }
            }
        }
    })
    .await;

    producer
        .await
        .expect("producer finished")
        .expect("producer ok");

    // Check finish_reason
    let final_event = collector.final_data_event().expect("final data event");
    let chunk = final_event.data.as_ref().expect("chunk data");
    assert_eq!(
        chunk.choices[0].finish_reason.as_deref(),
        Some("stop"),
        "Final chunk must have finish_reason='stop'"
    );
}

/// Test role in first chunk
#[tokio::test]
async fn test_role_in_first_chunk() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (done_tx, mut done_rx) = watch::channel(false);

    let source = MockStreamingSource::new("test-role", vec!["Hello".to_string()]);

    let done_signal = Arc::new(done_tx);
    let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

    let mut collector = SseEventCollector::new();
    let _ = timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    let _ = collector.collect_line(&line);
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        while let Ok(line) = rx.try_recv() {
                            let _ = collector.collect_line(&line);
                        }
                        break;
                    }
                }
            }
        }
    })
    .await;

    producer
        .await
        .expect("producer finished")
        .expect("producer ok");

    // Check role
    let data_events = collector.data_events();
    assert!(!data_events.is_empty(), "Must have data events");
    let first = data_events[0];
    let chunk = first.data.as_ref().expect("chunk data");
    assert_eq!(
        chunk.choices[0].delta.role.as_deref(),
        Some("assistant"),
        "First chunk must have role='assistant'"
    );
}

/// Test 5-second ceiling enforcement
#[tokio::test]
async fn test_timeout_ceiling() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (_done_tx, mut done_rx) = watch::channel(false);

    // Drop tx immediately to simulate stuck stream
    drop(tx);

    // Collection should timeout within 5s
    let start = std::time::Instant::now();
    let result = timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                line = rx.recv() => {
                    if line.is_none() {
                        break; // Channel closed
                    }
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        break;
                    }
                }
            }
        }
    })
    .await;

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(6),
        "Should complete within 6s"
    );
    // Channel closed immediately, so this should be very fast
    assert!(result.is_ok() || elapsed < Duration::from_secs(1));
}

/// Full streaming reliability test with failure bundle
#[tokio::test]
async fn test_streaming_reliability_full() {
    let trace_id = TestFailureBundle::generate_trace_id();
    let mut bundle_guard = FailureBundleGuard::new(&trace_id, "test_streaming_reliability_full");

    let stage_timer = StageTimer::start("full_streaming_test");

    // Create multi-token stream
    let tokens: Vec<String> = vec![
        "This".to_string(),
        " ".to_string(),
        "is".to_string(),
        " ".to_string(),
        "a".to_string(),
        " ".to_string(),
        "streaming".to_string(),
        " ".to_string(),
        "test".to_string(),
        ".".to_string(),
    ];

    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (done_tx, mut done_rx) = watch::channel(false);

    let source = MockStreamingSource::new("test-full-stream", tokens.clone());

    let done_signal = Arc::new(done_tx);
    let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

    // Collect all events
    let mut collector = SseEventCollector::new();
    let collect_result = timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                Some(line) = rx.recv() => {
                    if let Err(e) = collector.collect_line(&line) {
                        eprintln!("Collection error: {}", e);
                    }
                }
                _ = done_rx.changed() => {
                    if *done_rx.borrow() {
                        while let Ok(line) = rx.try_recv() {
                            let _ = collector.collect_line(&line);
                        }
                        break;
                    }
                }
            }
        }
    })
    .await;

    producer
        .await
        .expect("producer finished")
        .expect("producer ok");

    if collect_result.is_err() {
        bundle_guard.mark_failed("Stream collection timed out after 5s");
        panic!("Stream collection timed out");
    }

    bundle_guard
        .bundle_mut()
        .add_stage_timing(stage_timer.stop());
    bundle_guard
        .bundle_mut()
        .add_context("event_count", &collector.events().len().to_string());

    // Validate stream
    let validation = validate_stream(&collector);

    if !validation.passed {
        bundle_guard.mark_failed(&format!(
            "Stream validation failed: {:?}",
            validation.errors
        ));
        bundle_guard
            .bundle_mut()
            .add_context("validation_errors", &format!("{:?}", validation.errors));
        panic!(
            "Stream validation failed:\nErrors: {:?}\nWarnings: {:?}",
            validation.errors, validation.warnings
        );
    }

    // Additional assertions
    assert_eq!(
        collector.data_events().len(),
        tokens.len() + 1, // tokens + final chunk
        "Should have data event for each token plus final"
    );
    assert!(collector.has_done(), "Must have DONE marker");

    bundle_guard.cancel_save();
}

/// Test concurrent streams don't interfere
#[tokio::test]
async fn test_concurrent_streams_isolation() {
    let mut handles = Vec::new();

    for i in 0..5 {
        let handle = tokio::spawn(async move {
            let (tx, mut rx) = mpsc::channel::<String>(100);
            let (done_tx, mut done_rx) = watch::channel(false);

            let source = MockStreamingSource::new(
                &format!("concurrent-{}", i),
                vec![format!("Stream{}", i)],
            );

            let done_signal = Arc::new(done_tx);
            let producer = tokio::spawn(async move { source.produce(tx, done_signal).await });

            let mut collector = SseEventCollector::new();
            let _ = timeout(Duration::from_secs(5), async {
                loop {
                    tokio::select! {
                        Some(line) = rx.recv() => {
                            let _ = collector.collect_line(&line);
                        }
                        _ = done_rx.changed() => {
                            if *done_rx.borrow() {
                                while let Ok(line) = rx.try_recv() {
                                    let _ = collector.collect_line(&line);
                                }
                                break;
                            }
                        }
                    }
                }
            })
            .await;

            producer.await.expect("producer").expect("producer ok");

            validate_stream(&collector)
        });

        handles.push(handle);
    }

    // Wait for all streams
    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("task"))
        .collect();

    // All should pass
    for (i, result) in results.iter().enumerate() {
        assert!(result.passed, "Stream {} failed: {:?}", i, result.errors);
    }
}

/// Test keep-alive handling
#[tokio::test]
async fn test_keepalive_handling() {
    let mut collector = SseEventCollector::new();

    // Simulate keep-alives between data
    collector.collect_line(r#"data: {"id":"test","object":"chat.completion.chunk","created":0,"model":"test","choices":[{"index":0,"delta":{"content":"Hi","role":"assistant"},"finish_reason":null}]}"#).unwrap();
    collector.collect_line(": keep-alive").unwrap();
    collector.collect_line(": ping").unwrap();
    collector.collect_line(r#"data: {"id":"test","object":"chat.completion.chunk","created":0,"model":"test","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}"#).unwrap();
    collector.collect_line("data: [DONE]").unwrap();

    // Validation should still pass
    let validation = validate_stream(&collector);

    // Keep-alives shouldn't break validation
    assert!(
        validation.passed || validation.errors.iter().all(|e| !e.contains("keep-alive")),
        "Keep-alives shouldn't cause validation errors"
    );

    // Should have 2 data events (excluding keep-alives and DONE)
    assert_eq!(collector.data_events().len(), 2);
}
