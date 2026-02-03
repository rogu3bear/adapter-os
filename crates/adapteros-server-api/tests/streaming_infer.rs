//! Streaming inference endpoint tests
//!
//! Tests for SSE-based token streaming with MLX backend simulation.
//! Validates proper SSE formatting, client disconnect handling, and latency characteristics.
//!
//! Target metrics:
//! - Token generation latency: ~0.39ms per token (MLX speed)
//! - First token latency: <100ms
//! - SSE formatting overhead: <1ms per event
//!
//! [2025-11-22 streaming_inference_tests]

use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::{derive_seed, B3Hash};
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::traits::CreateStackRequest;
use adapteros_server_api::handlers::streaming_infer::{streaming_infer, StreamingInferRequest};
use axum::body::to_bytes;
use axum::response::IntoResponse;
use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

mod common;
use common::{create_test_adapter_default, register_test_worker, setup_state, test_admin_claims};

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
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Token generation event for internal streaming
#[derive(Debug, Clone)]
pub struct TokenEvent {
    pub token_id: u32,
    pub text: String,
    pub delta_us: u64,
    pub elapsed_us: u64,
}

/// Streaming completion event
#[derive(Debug, Clone)]
pub enum StreamingEvent {
    Token(TokenEvent),
    Done {
        finish_reason: String,
        total_tokens: usize,
        total_time_us: u64,
        tokens_per_sec: f32,
    },
    Error {
        message: String,
        code: String,
    },
    KeepAlive,
}

/// Format a streaming event as an SSE message in OpenAI-compatible format
pub fn format_sse_event(event: &StreamingEvent, request_id: &str) -> String {
    match event {
        StreamingEvent::Token(token) => {
            let chunk = ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                model: "adapteros-mlx".to_string(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: ChunkDelta {
                        content: Some(token.text.clone()),
                        role: None,
                    },
                    finish_reason: None,
                }],
            };

            format!(
                "data: {}\n\n",
                serde_json::to_string(&chunk).unwrap_or_default()
            )
        }
        StreamingEvent::Done { finish_reason, .. } => {
            let chunk = ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                model: "adapteros-mlx".to_string(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: ChunkDelta {
                        content: None,
                        role: None,
                    },
                    finish_reason: Some(finish_reason.clone()),
                }],
            };

            format!(
                "data: {}\n\ndata: [DONE]\n\n",
                serde_json::to_string(&chunk).unwrap_or_default()
            )
        }
        StreamingEvent::Error { message, code } => {
            let error = serde_json::json!({
                "error": {
                    "message": message,
                    "type": code,
                    "code": code
                }
            });

            format!(
                "data: {}\n\n",
                serde_json::to_string(&error).unwrap_or_default()
            )
        }
        StreamingEvent::KeepAlive => ": keep-alive\n\n".to_string(),
    }
}

/// Simulate MLX token generation at realistic speed
/// Target: ~0.39ms per token (based on MLX benchmarks)
pub struct MLXTokenSimulator {
    base_seed: B3Hash,
    token_delay_us: u64,
    tokens: Vec<(u32, String)>,
    current_index: usize,
}

impl MLXTokenSimulator {
    /// Create new simulator with MLX-realistic timing
    pub fn new(base_seed: B3Hash) -> Self {
        // Simulate realistic token vocabulary
        let tokens = vec![
            (1, "Hello".to_string()),
            (2, " ".to_string()),
            (3, "world".to_string()),
            (4, "!".to_string()),
            (5, " ".to_string()),
            (6, "This".to_string()),
            (7, " ".to_string()),
            (8, "is".to_string()),
            (9, " ".to_string()),
            (10, "a".to_string()),
            (11, " ".to_string()),
            (12, "streaming".to_string()),
            (13, " ".to_string()),
            (14, "test".to_string()),
            (15, ".".to_string()),
        ];

        Self {
            base_seed,
            token_delay_us: 390, // 0.39ms per token
            tokens,
            current_index: 0,
        }
    }

    /// Generate next token with simulated delay
    pub async fn next_token(&mut self, step: usize) -> Option<TokenEvent> {
        if self.current_index >= self.tokens.len() {
            return None;
        }

        // Simulate token generation delay
        tokio::time::sleep(Duration::from_micros(self.token_delay_us)).await;

        let (token_id, text) = self.tokens[self.current_index].clone();
        self.current_index += 1;

        // Derive step-specific seed for determinism tracking
        let _step_seed = derive_seed(&self.base_seed, &format!("mlx-sim-step:{}", step));

        Some(TokenEvent {
            token_id,
            text,
            delta_us: self.token_delay_us,
            elapsed_us: (self.current_index as u64) * self.token_delay_us,
        })
    }

    pub fn reset(&mut self) {
        self.current_index = 0;
    }
}

/// Streaming state for testing client disconnections
pub struct StreamingState {
    pub client_connected: Arc<AtomicBool>,
    pub tokens_sent: Arc<AtomicU64>,
    pub start_time: Instant,
}

impl StreamingState {
    pub fn new() -> Self {
        Self {
            client_connected: Arc::new(AtomicBool::new(true)),
            tokens_sent: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
        }
    }

    pub fn disconnect(&self) {
        self.client_connected.store(false, Ordering::SeqCst);
    }

    pub fn is_connected(&self) -> bool {
        self.client_connected.load(Ordering::SeqCst)
    }

    pub fn record_token(&self) {
        self.tokens_sent.fetch_add(1, Ordering::SeqCst);
    }

    pub fn tokens_sent(&self) -> u64 {
        self.tokens_sent.load(Ordering::SeqCst)
    }
}

impl Default for StreamingState {
    fn default() -> Self {
        Self::new()
    }
}

/// Test that streaming inference emits structured error payloads with all required fields.
/// The error payload must contain: code, message, retryable (bool), correlation_id.
#[tokio::test]
async fn streaming_infer_emits_structured_error_on_unavailable_resource() {
    let state = setup_state(None).await.expect("state");
    let caps = WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    let worker_id = register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("register worker");
    let manifest_hash = format!("manifest-{}", worker_id);
    let state = state.with_manifest_info(manifest_hash, "mlx".to_string());

    let adapter_id = format!("adapter-test-{}", uuid::Uuid::new_v4());
    create_test_adapter_default(&state, &adapter_id, "tenant-1")
        .await
        .expect("create adapter");

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test".to_string(),
    );

    let req = StreamingInferRequest {
        prompt: "hello".to_string(),
        model: None,
        backend: None,
        coreml_mode: None,
        routing_determinism_mode: None,
        stack_id: None,
        domain: None,
        max_tokens: 16,
        temperature: 0.7,
        top_p: None,
        top_k: None,
        stop: Vec::new(),
        adapter_stack: None,
        adapters: Some(vec![adapter_id]),
        seed: None,
        adapter_strength_overrides: None,
        require_evidence: false,
        reasoning_mode: false,
        collection_id: None,
        session_id: None,
        effective_adapter_ids: None,
        stop_policy: None,
        context: None,
    };

    let sse = streaming_infer(
        State(state),
        Extension(claims),
        Extension(identity),
        axum::http::HeaderMap::new(),
        None,
        Json(req),
    )
    .await
    .expect("sse response");
    let response = sse.into_response();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8 body");

    let mut error_payload: Option<Value> = None;
    for block in body_str.split("\n\n") {
        let mut event_type = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_type = Some(value.trim());
            } else if let Some(value) = line.strip_prefix("data:") {
                data = Some(value.trim());
            }
        }

        if event_type == Some("error") {
            if let Some(data) = data {
                error_payload = serde_json::from_str::<Value>(data).ok();
                break;
            }
        }
    }

    let payload = error_payload.expect("structured error payload");
    assert!(payload.get("code").is_some(), "missing error code");
    assert!(payload.get("message").is_some(), "missing error message");
    // Verify retryable field exists and is a boolean (value can be true or false depending on error type)
    assert!(
        payload.get("retryable").and_then(Value::as_bool).is_some(),
        "missing or invalid retryable field, got: {:?}",
        payload.get("retryable")
    );
    assert!(
        payload
            .get("correlation_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .starts_with("chatcmpl-"),
        "expected correlation_id"
    );
}

/// Session-backed routing should resolve effective adapters from session metadata when
/// the request omits explicit adapters.
#[tokio::test]
async fn streaming_infer_resolves_effective_adapters_from_session_stack() {
    let state = setup_state(None).await.expect("state");
    {
        let mut config = state.config.write().expect("config write");
        config.use_session_stack_for_routing = true;
    }

    let caps = WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    let worker_id = register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("register worker");
    let manifest_hash = format!("manifest-{}", worker_id);
    let state = state.with_manifest_info(manifest_hash, "mlx".to_string());

    let adapter_id = format!("adapter-session-stack-{}", uuid::Uuid::new_v4());
    create_test_adapter_default(&state, &adapter_id, "tenant-1")
        .await
        .expect("create adapter");

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test".to_string(),
    );

    let stack_req = CreateStackRequest {
        tenant_id: claims.tenant_id.clone(),
        name: format!("stack.session.{}", uuid::Uuid::new_v4().simple()),
        description: None,
        adapter_ids: vec![adapter_id.clone()],
        workflow_type: Some("Parallel".to_string()),
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.expect("create stack");

    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    state
        .db
        .create_chat_session(CreateChatSessionParams {
            id: session_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            user_id: Some(claims.sub.clone()),
            created_by: Some(claims.sub.clone()),
            stack_id: Some(stack_id),
            collection_id: None,
            document_id: None,
            name: "Session Stack".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
            codebase_adapter_id: None,
        })
        .await
        .expect("create session");

    let req = StreamingInferRequest {
        prompt: "hello".to_string(),
        model: None,
        backend: None,
        coreml_mode: None,
        routing_determinism_mode: None,
        stack_id: None,
        domain: None,
        max_tokens: 16,
        temperature: 0.7,
        top_p: None,
        top_k: None,
        stop: Vec::new(),
        adapter_stack: None,
        adapters: None,
        seed: None,
        adapter_strength_overrides: None,
        require_evidence: false,
        reasoning_mode: false,
        collection_id: None,
        session_id: Some(session_id),
        effective_adapter_ids: None,
        stop_policy: None,
        context: None,
    };

    let sse = streaming_infer(
        State(state),
        Extension(claims),
        Extension(identity),
        axum::http::HeaderMap::new(),
        None,
        Json(req),
    )
    .await
    .expect("sse response");
    let response = sse.into_response();
    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("read body");
    let body_str = String::from_utf8(body.to_vec()).expect("utf8 body");

    assert!(
        body_str.contains("data:") || body_str.contains("event:"),
        "expected SSE response, got: {}",
        body_str
    );

    let mut error_message: Option<String> = None;
    for block in body_str.split("\n\n") {
        let mut event_type = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("event:") {
                event_type = Some(value.trim());
            } else if let Some(value) = line.strip_prefix("data:") {
                data = Some(value.trim());
            }
        }

        if event_type == Some("error") {
            if let Some(data) = data {
                if let Ok(payload) = serde_json::from_str::<Value>(data) {
                    if let Some(message) = payload.get("message").and_then(Value::as_str) {
                        error_message = Some(message.to_string());
                        break;
                    }
                }
            }
        }
    }

    if let Some(message) = error_message {
        let lowered = message.to_lowercase();
        assert!(
            !lowered.contains("session not found"),
            "unexpected session lookup failure: {}",
            message
        );
        assert!(
            !lowered.contains("must specify adapters"),
            "unexpected adapter requirement: {}",
            message
        );
    }
}

/// Run streaming generation with proper client disconnect handling
pub async fn run_streaming_generation(
    mut simulator: MLXTokenSimulator,
    state: Arc<StreamingState>,
    tx: mpsc::Sender<StreamingEvent>,
) -> Result<(), String> {
    let mut step = 0;
    let start = Instant::now();

    while state.is_connected() {
        match simulator.next_token(step).await {
            Some(token) => {
                // Check if client is still connected before sending
                if !state.is_connected() {
                    tracing::debug!("Client disconnected during streaming");
                    break;
                }

                if tx.send(StreamingEvent::Token(token)).await.is_err() {
                    // Client disconnected (channel closed)
                    tracing::debug!("Channel closed - client disconnected");
                    break;
                }

                state.record_token();
                step += 1;
            }
            None => {
                // Generation complete
                let elapsed_us = start.elapsed().as_micros() as u64;
                let tokens = state.tokens_sent();
                let tokens_per_sec = if elapsed_us > 0 {
                    (tokens as f32) / (elapsed_us as f32 / 1_000_000.0)
                } else {
                    0.0
                };

                let _ = tx
                    .send(StreamingEvent::Done {
                        finish_reason: "stop".to_string(),
                        total_tokens: tokens as usize,
                        total_time_us: elapsed_us,
                        tokens_per_sec,
                    })
                    .await;
                break;
            }
        }
    }

    Ok(())
}

/// Streaming latency metrics
#[derive(Debug, Clone, Default)]
pub struct StreamingLatencyMetrics {
    pub first_token_latency_us: u64,
    pub avg_token_latency_us: u64,
    pub p99_token_latency_us: u64,
    pub total_tokens: usize,
    pub total_time_us: u64,
    pub tokens_per_second: f32,
    pub sse_overhead_us: u64,
}

impl StreamingLatencyMetrics {
    pub fn from_token_latencies(latencies: &[u64]) -> Self {
        if latencies.is_empty() {
            return Self::default();
        }

        let mut sorted = latencies.to_vec();
        sorted.sort_unstable();

        let first = latencies.first().copied().unwrap_or(0);
        let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
        let p99_idx = (latencies.len() as f64 * 0.99) as usize;
        let p99 = sorted
            .get(p99_idx.min(sorted.len() - 1))
            .copied()
            .unwrap_or(0);
        let total = latencies.iter().sum::<u64>();

        let tokens_per_sec = if total > 0 {
            (latencies.len() as f32) / (total as f32 / 1_000_000.0)
        } else {
            0.0
        };

        Self {
            first_token_latency_us: first,
            avg_token_latency_us: avg,
            p99_token_latency_us: p99,
            total_tokens: latencies.len(),
            total_time_us: total,
            tokens_per_second: tokens_per_sec,
            sse_overhead_us: 0, // Set separately
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sse_format_token_event() {
        let event = StreamingEvent::Token(TokenEvent {
            token_id: 42,
            text: "Hello".to_string(),
            delta_us: 390,
            elapsed_us: 390,
        });

        let sse = format_sse_event(&event, "chatcmpl-test");

        assert!(sse.starts_with("data: "), "SSE must start with 'data: '");
        assert!(sse.ends_with("\n\n"), "SSE must end with double newline");
        assert!(sse.contains("Hello"), "SSE must contain token text");
        assert!(
            sse.contains("chat.completion.chunk"),
            "SSE must have correct object type"
        );
        assert!(sse.contains("adapteros-mlx"), "SSE must include model name");
    }

    #[tokio::test]
    async fn test_sse_format_done_event() {
        let event = StreamingEvent::Done {
            finish_reason: "stop".to_string(),
            total_tokens: 100,
            total_time_us: 39000,
            tokens_per_sec: 2564.1,
        };

        let sse = format_sse_event(&event, "chatcmpl-test");

        assert!(
            sse.contains("[DONE]"),
            "Done event must include [DONE] marker"
        );
        assert!(
            sse.contains("stop"),
            "Done event must include finish_reason"
        );
    }

    #[tokio::test]
    async fn test_sse_format_keep_alive() {
        let event = StreamingEvent::KeepAlive;
        let sse = format_sse_event(&event, "test");

        assert_eq!(
            sse, ": keep-alive\n\n",
            "Keep-alive must be SSE comment format"
        );
    }

    #[tokio::test]
    async fn test_mlx_token_simulator_timing() {
        let base_seed = B3Hash::hash(b"test-seed");
        let mut simulator = MLXTokenSimulator::new(base_seed);

        let start = Instant::now();
        let mut token_count = 0;

        while let Some(_token) = simulator.next_token(token_count).await {
            token_count += 1;
        }

        let elapsed = start.elapsed();
        let expected_min =
            Duration::from_micros(simulator.token_delay_us * (token_count as u64 - 1));

        assert!(
            elapsed >= expected_min,
            "Simulator should maintain minimum delay: {:?} < {:?}",
            elapsed,
            expected_min
        );
        assert!(token_count > 0, "Should generate at least one token");
    }

    #[tokio::test]
    async fn test_streaming_generation_complete() {
        let base_seed = B3Hash::hash(b"test-stream");
        let simulator = MLXTokenSimulator::new(base_seed);
        let state = Arc::new(StreamingState::new());
        let (tx, mut rx) = mpsc::channel(100);

        let state_clone = state.clone();
        let handle =
            tokio::spawn(async move { run_streaming_generation(simulator, state_clone, tx).await });

        let mut received_tokens = Vec::new();
        let mut received_done = false;

        while let Some(event) = rx.recv().await {
            match event {
                StreamingEvent::Token(t) => received_tokens.push(t),
                StreamingEvent::Done { .. } => {
                    received_done = true;
                    break;
                }
                _ => {}
            }
        }

        handle.await.unwrap().unwrap();

        assert!(!received_tokens.is_empty(), "Should receive tokens");
        assert!(received_done, "Should receive done event");
        assert_eq!(
            state.tokens_sent() as usize,
            received_tokens.len(),
            "Token count should match"
        );
    }

    #[tokio::test]
    async fn test_client_disconnect_handling() {
        let base_seed = B3Hash::hash(b"test-disconnect");
        let simulator = MLXTokenSimulator::new(base_seed);
        let state = Arc::new(StreamingState::new());
        let (tx, mut rx) = mpsc::channel(100);

        let state_clone = state.clone();
        let handle =
            tokio::spawn(async move { run_streaming_generation(simulator, state_clone, tx).await });

        // Receive first token
        let first_event = rx.recv().await;
        assert!(matches!(first_event, Some(StreamingEvent::Token(_))));

        // Simulate client disconnect after receiving first token
        state.disconnect();
        drop(rx);

        // Generation should stop gracefully
        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "Generation should handle disconnect gracefully"
        );

        // Should have stopped early due to disconnect
        assert!(
            state.tokens_sent() < 15,
            "Should stop before all tokens sent"
        );
    }

    #[tokio::test]
    async fn test_streaming_latency_metrics() {
        let latencies = vec![400, 390, 385, 395, 405, 380, 410, 390, 395, 400];
        let metrics = StreamingLatencyMetrics::from_token_latencies(&latencies);

        assert_eq!(metrics.first_token_latency_us, 400);
        assert_eq!(metrics.total_tokens, 10);
        assert!(metrics.avg_token_latency_us > 380 && metrics.avg_token_latency_us < 410);
        assert!(metrics.p99_token_latency_us >= 400);
        assert!(metrics.tokens_per_second > 2000.0);
    }

    #[tokio::test]
    async fn test_deterministic_seed_derivation() {
        let base_seed = B3Hash::hash(b"test-determinism");

        // Same seed should produce same derived seed
        let seed1 = derive_seed(&base_seed, "mlx-sim-step:5");
        let seed2 = derive_seed(&base_seed, "mlx-sim-step:5");
        assert_eq!(seed1, seed2, "Same step should produce same seed");

        // Different steps should produce different seeds
        let seed3 = derive_seed(&base_seed, "mlx-sim-step:6");
        assert_ne!(
            seed1, seed3,
            "Different steps should produce different seeds"
        );
    }

    #[tokio::test]
    async fn test_sse_formatting_overhead() {
        let event = StreamingEvent::Token(TokenEvent {
            token_id: 42,
            text: "test".to_string(),
            delta_us: 390,
            elapsed_us: 1000,
        });

        // Measure SSE formatting time
        let iterations = 1000;
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = format_sse_event(&event, "chatcmpl-test");
        }
        let elapsed = start.elapsed();

        let avg_ns = elapsed.as_nanos() / iterations as u128;
        let avg_us = avg_ns as f64 / 1000.0;

        // SSE formatting should be under 100us on average
        assert!(
            avg_us < 100.0,
            "SSE formatting overhead too high: {}us",
            avg_us
        );

        println!("SSE formatting overhead: {:.2}us per event", avg_us);
    }

    #[tokio::test]
    async fn test_openai_compatible_format() {
        let event = StreamingEvent::Token(TokenEvent {
            token_id: 1,
            text: "Hello".to_string(),
            delta_us: 390,
            elapsed_us: 390,
        });

        let sse = format_sse_event(&event, "chatcmpl-abc123");

        // Parse the SSE data
        let data_line = sse.strip_prefix("data: ").unwrap().trim_end();
        let chunk: ChatCompletionChunk = serde_json::from_str(data_line).unwrap();

        // Verify OpenAI-compatible structure
        assert_eq!(chunk.id, "chatcmpl-abc123");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.model, "adapteros-mlx");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].index, 0);
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[tokio::test]
    async fn test_streaming_throughput_benchmark() {
        let base_seed = B3Hash::hash(b"test-throughput");
        let mut simulator = MLXTokenSimulator::new(base_seed);

        let start = Instant::now();
        let mut latencies = Vec::new();
        let mut prev_time = start;

        while let Some(_token) = simulator.next_token(latencies.len()).await {
            let now = Instant::now();
            latencies.push(now.duration_since(prev_time).as_micros() as u64);
            prev_time = now;
        }

        let total_elapsed = start.elapsed();
        let metrics = StreamingLatencyMetrics::from_token_latencies(&latencies);

        println!("Streaming throughput benchmark:");
        println!("  Total tokens: {}", metrics.total_tokens);
        println!("  Total time: {:?}", total_elapsed);
        println!(
            "  First token latency: {}us",
            metrics.first_token_latency_us
        );
        println!("  Avg token latency: {}us", metrics.avg_token_latency_us);
        println!("  P99 token latency: {}us", metrics.p99_token_latency_us);
        println!("  Tokens/sec: {:.1}", metrics.tokens_per_second);

        // Verify throughput is in expected range
        // With simulated 0.39ms delay per token, throughput is ~2500 tok/s theoretical
        // In practice, timing overhead reduces this to ~500-1500 tok/s
        assert!(
            metrics.tokens_per_second > 100.0,
            "Throughput too low: {} tokens/sec (expected >100)",
            metrics.tokens_per_second
        );
        assert!(
            metrics.tokens_per_second < 5000.0,
            "Throughput unrealistically high: {} tokens/sec",
            metrics.tokens_per_second
        );
    }

    #[tokio::test]
    async fn test_error_event_format() {
        let event = StreamingEvent::Error {
            message: "Model not loaded".to_string(),
            code: "model_error".to_string(),
        };

        let sse = format_sse_event(&event, "test");

        assert!(sse.starts_with("data: "));
        assert!(sse.contains("Model not loaded"));
        assert!(sse.contains("model_error"));
    }

    #[tokio::test]
    async fn test_concurrent_streaming_sessions() {
        let base_seed = B3Hash::hash(b"test-concurrent");

        let mut handles = Vec::new();

        // Start 5 concurrent streaming sessions
        for i in 0..5 {
            let session_seed = derive_seed(&base_seed, &format!("session:{}", i));
            let simulator = MLXTokenSimulator::new(B3Hash::from_bytes(session_seed));
            let state = Arc::new(StreamingState::new());
            let (tx, mut rx) = mpsc::channel(100);

            let state_clone = state.clone();
            let gen_handle =
                tokio::spawn(
                    async move { run_streaming_generation(simulator, state_clone, tx).await },
                );

            let recv_handle = tokio::spawn(async move {
                let mut tokens = 0;
                while let Some(event) = rx.recv().await {
                    match event {
                        StreamingEvent::Token(_) => tokens += 1,
                        StreamingEvent::Done { .. } => break,
                        _ => {}
                    }
                }
                tokens
            });

            handles.push((gen_handle, recv_handle));
        }

        // Wait for all sessions to complete
        let mut total_tokens = 0;
        for (gen_handle, recv_handle) in handles {
            gen_handle.await.unwrap().unwrap();
            total_tokens += recv_handle.await.unwrap();
        }

        // Each session should generate all tokens
        assert_eq!(total_tokens, 5 * 15, "All sessions should complete");
    }
}
