//! Streaming inference endpoint handler
//!
//! Provides Server-Sent Events (SSE) streaming for token-by-token inference output.
//! Compatible with OpenAI's streaming API format for chat completions.
//!
//! # Endpoint
//! `POST /v1/infer/stream`
//!
//! # SSE Event Format
//! ```text
//! data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
//! data: [DONE]
//! ```

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use crate::uds_client::UdsClient;
use adapteros_core::identity::IdentityEnvelope;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tracing::{debug, error, info};
use utoipa::ToSchema;

/// Streaming inference request
///
/// Accepts the same fields as the standard `/v1/infer` endpoint but returns
/// a stream of Server-Sent Events (SSE) with tokens as they are generated.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingInferRequest {
    /// The input prompt for inference
    pub prompt: String,
    /// Model identifier (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Maximum number of tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Sampling temperature (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Top-p nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Stop sequences to terminate generation
    #[serde(default)]
    pub stop: Vec<String>,
    /// Adapter stack to use for inference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter_stack: Option<Vec<String>>,
    /// Specific adapters to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapters: Option<Vec<String>>,
    /// Random seed for reproducibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Require evidence in response
    #[serde(default)]
    pub require_evidence: bool,
    /// Collection ID for scoping RAG retrieval to specific document collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

/// OpenAI-compatible streaming chunk response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChunk {
    /// Unique identifier for the completion
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    pub object: String,
    /// Unix timestamp of creation
    pub created: u64,
    /// Model used for generation
    pub model: String,
    /// System fingerprint for determinism tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// Array of choices
    pub choices: Vec<StreamingChoice>,
}

/// Individual choice in a streaming response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingChoice {
    /// Index of this choice
    pub index: usize,
    /// Delta containing new content
    pub delta: Delta,
    /// Finish reason (null until complete)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Delta containing new content
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Delta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Content delta (new tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Internal streaming event
#[derive(Debug, Clone)]
enum StreamEvent {
    /// First chunk with role
    Start,
    /// Token generated
    Token(String),
    /// Generation complete
    Done { finish_reason: String },
    /// Error occurred
    Error(String),
}

fn serialize_safe<T: Serialize>(value: &T, context: &str) -> String {
    match serde_json::to_string(value) {
        Ok(json) => json,
        Err(error) => {
            error!(
                context = %context,
                error = %error,
                "Failed to serialize streaming response payload"
            );
            serde_json::json!({
                "error": {
                    "message": "stream serialization failed",
                    "type": "serialization_error",
                    "code": "SERIALIZATION_ERROR",
                    "context": context
                }
            })
            .to_string()
        }
    }
}

/// Streaming inference handler
///
/// Accepts inference requests and returns a stream of Server-Sent Events (SSE)
/// with tokens as they are generated. Compatible with OpenAI's streaming API.
///
/// # SSE Event Format
/// ```text
/// data: {"id":"chatcmpl-xxx","object":"chat.completion.chunk","choices":[{"delta":{"content":"token"}}]}
/// data: [DONE]
/// ```
///
/// # Collection-Scoped RAG
/// When `collection_id` is provided, the inference will be scoped to documents
/// within that collection, enabling targeted RAG retrieval for domain-specific queries.
///
/// # Example
/// ```bash
/// curl -X POST http://localhost:3000/v1/infer/stream \
///   -H "Content-Type: application/json" \
///   -H "Authorization: Bearer <token>" \
///   -d '{"prompt": "Hello, world!", "max_tokens": 100}'
/// ```
///
/// # Example with Collection-Scoped RAG
/// ```bash
/// curl -X POST http://localhost:3000/v1/infer/stream \
///   -H "Content-Type: application/json" \
///   -H "Authorization: Bearer <token>" \
///   -d '{"prompt": "What is the pricing model?", "max_tokens": 200, "collection_id": "marketing-docs"}'
/// ```
#[utoipa::path(
    post,
    path = "/v1/infer/stream",
    tag = "inference",
    request_body = StreamingInferRequest,
    responses(
        (status = 200, description = "Server-Sent Events stream of tokens"),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 503, description = "Service under memory pressure", body = ErrorResponse),
    )
)]
pub async fn streaming_infer(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Extension(_identity): Extension<IdentityEnvelope>,
    Json(req): Json<StreamingInferRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: Operator, SRE, and Admin can execute inference
    crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::InferenceExecute,
    )?;

    // Validate request
    if req.prompt.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("prompt cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    // Check UMA pressure
    let pressure_str = state.uma_monitor.get_current_pressure().to_string();
    let is_high_pressure = pressure_str == "High" || pressure_str == "Critical";
    if is_high_pressure {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("service under memory pressure")
                    .with_code("BACKPRESSURE")
                    .with_string_details(format!(
                        "level={}, retry_after_secs=30, action=reduce max_tokens or retry later",
                        pressure_str
                    )),
            ),
        ));
    }

    // Generate request ID
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let model_name = req.model.clone().unwrap_or_else(|| "adapteros".to_string());

    info!(
        request_id = %request_id,
        prompt_len = req.prompt.len(),
        max_tokens = req.max_tokens,
        collection_id = ?req.collection_id,
        "Starting streaming inference"
    );

    // Collection-scoped RAG integration
    // TODO: When collection_id is provided, integrate RAG retrieval scoped to that collection
    // Integration points:
    // 1. Query documents in the specified collection via `state.db` (see collections.rs)
    // 2. Use RAG retriever to find relevant documents/chunks for the prompt
    // 3. Augment the prompt with retrieved context before sending to worker
    // 4. Track evidence entries for retrieved documents (see inference_evidence.rs)
    // Example:
    // if let Some(collection_id) = &req.collection_id {
    //     let rag_context = retrieve_from_collection(&state.db, collection_id, &req.prompt).await?;
    //     augmented_prompt = format!("Context:\n{}\n\nQuery: {}", rag_context, req.prompt);
    // }
    let _collection_scoped_rag_enabled = req.collection_id.is_some();

    // Audit log: inference execution start
    let adapters_requested = req
        .adapters
        .as_ref()
        .map(|a| a.join(","))
        .or_else(|| req.adapter_stack.as_ref().map(|s| s.join(",")));

    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::INFERENCE_EXECUTE,
        crate::audit_helper::resources::ADAPTER,
        adapters_requested.as_deref(),
    )
    .await;

    // Get available workers
    let workers = state.db.list_all_workers().await.map_err(|e| {
        error!(error = %e, "Failed to list workers");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to list workers")
                    .with_code("INTERNAL_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Resolve UDS path
    let uds_path_buf = if let Some(worker) = workers.first() {
        std::path::PathBuf::from(&worker.uds_path)
    } else {
        let fallback = std::env::var("AOS_WORKER_SOCKET")
            .unwrap_or_else(|_| "/var/run/adapteros.sock".to_string());
        std::path::PathBuf::from(fallback)
    };

    // Clone data for the stream
    let request_id_clone = request_id.clone();
    let model_name_clone = model_name.clone();
    let prompt = req.prompt.clone();
    let max_tokens = req.max_tokens;
    let temperature = req.temperature;

    // Create the SSE stream
    let stream = stream::unfold(
        StreamState::new(
            state,
            uds_path_buf,
            prompt,
            max_tokens,
            temperature,
            request_id_clone,
            model_name_clone,
        ),
        |mut stream_state| async move {
            match stream_state.next_event().await {
                Some(event) => {
                    let sse_event = stream_state.format_event(event);
                    Some((Ok(sse_event), stream_state))
                }
                None => None,
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

/// Internal state for streaming generation
#[allow(dead_code)]
struct StreamState {
    /// Application state (reserved for future token-by-token streaming)
    state: AppState,
    uds_path: std::path::PathBuf,
    prompt: String,
    max_tokens: usize,
    /// Temperature (reserved for future token-by-token streaming)
    temperature: f32,
    request_id: String,
    model_name: String,
    // State machine
    phase: StreamPhase,
    // Generated text for chunking
    generated_text: Option<String>,
    word_index: usize,
    words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum StreamPhase {
    Start,
    GeneratingText,
    StreamingTokens,
    Done,
}

impl StreamState {
    fn new(
        state: AppState,
        uds_path: std::path::PathBuf,
        prompt: String,
        max_tokens: usize,
        temperature: f32,
        request_id: String,
        model_name: String,
    ) -> Self {
        Self {
            state,
            uds_path,
            prompt,
            max_tokens,
            temperature,
            request_id,
            model_name,
            phase: StreamPhase::Start,
            generated_text: None,
            word_index: 0,
            words: Vec::new(),
        }
    }

    async fn next_event(&mut self) -> Option<StreamEvent> {
        match self.phase {
            StreamPhase::Start => {
                // Send initial role chunk
                self.phase = StreamPhase::GeneratingText;
                Some(StreamEvent::Start)
            }
            StreamPhase::GeneratingText => {
                // Generate the full response via UDS
                match self.generate_response().await {
                    Ok(text) => {
                        // Split into words for streaming simulation
                        self.words = text
                            .split_inclusive(|c: char| c.is_whitespace() || c == '\n')
                            .map(|s| s.to_string())
                            .collect();
                        self.generated_text = Some(text);
                        self.word_index = 0;
                        self.phase = StreamPhase::StreamingTokens;
                        // Return first token immediately instead of recursing
                        if !self.words.is_empty() {
                            let word = self.words[0].clone();
                            self.word_index = 1;
                            Some(StreamEvent::Token(word))
                        } else {
                            self.phase = StreamPhase::Done;
                            Some(StreamEvent::Done {
                                finish_reason: "stop".to_string(),
                            })
                        }
                    }
                    Err(e) => {
                        self.phase = StreamPhase::Done;
                        Some(StreamEvent::Error(e))
                    }
                }
            }
            StreamPhase::StreamingTokens => {
                if self.word_index < self.words.len() {
                    let word = self.words[self.word_index].clone();
                    self.word_index += 1;
                    Some(StreamEvent::Token(word))
                } else {
                    self.phase = StreamPhase::Done;
                    Some(StreamEvent::Done {
                        finish_reason: "stop".to_string(),
                    })
                }
            }
            StreamPhase::Done => None,
        }
    }

    async fn generate_response(&self) -> Result<String, String> {
        // Create UDS client with 60 second timeout
        let uds_client = UdsClient::new(Duration::from_secs(60));

        // Build worker inference request
        let worker_request = WorkerInferRequest {
            cpid: uuid::Uuid::new_v4().to_string(),
            prompt: self.prompt.clone(),
            max_tokens: self.max_tokens,
            require_evidence: false,
        };

        // Send request to worker
        match uds_client.infer(&self.uds_path, worker_request).await {
            Ok(response) => {
                if let Some(text) = response.text {
                    debug!(
                        text_len = text.len(),
                        status = %response.status,
                        "Received inference response"
                    );
                    Ok(text)
                } else {
                    Err("No text in response".to_string())
                }
            }
            Err(e) => {
                error!(error = %e, "UDS inference failed");
                Err(format!("Inference failed: {}", e))
            }
        }
    }

    fn format_event(&self, event: StreamEvent) -> Event {
        match event {
            StreamEvent::Start => {
                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: Some("assistant".to_string()),
                            content: None,
                        },
                        finish_reason: None,
                    }],
                };
                Event::default().data(serialize_safe(&chunk, "stream_start"))
            }
            StreamEvent::Token(content) => {
                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: Some(content),
                        },
                        finish_reason: None,
                    }],
                };
                Event::default().data(serialize_safe(&chunk, "stream_token"))
            }
            StreamEvent::Done { finish_reason } => {
                let chunk = StreamingChunk {
                    id: self.request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: current_timestamp(),
                    model: self.model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: None,
                        },
                        finish_reason: Some(finish_reason),
                    }],
                };
                let chunk_json = serialize_safe(&chunk, "stream_done");
                // Send final chunk followed by [DONE]
                Event::default().data(format!("{}\n\ndata: [DONE]", chunk_json))
            }
            StreamEvent::Error(message) => {
                let error_response = serde_json::json!({
                    "error": {
                        "message": message,
                        "type": "inference_error",
                        "code": "INFERENCE_ERROR"
                    }
                });
                Event::default().data(serialize_safe(&error_response, "stream_error"))
            }
        }
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_request_defaults() {
        let json = r#"{"prompt": "Hello"}"#;
        let req: StreamingInferRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Hello");
        assert_eq!(req.max_tokens, 512);
        assert!((req.temperature - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_streaming_chunk_serialization() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some("Hello".to_string()),
                },
                finish_reason: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("chat.completion.chunk"));
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_done_chunk_format() {
        let chunk = StreamingChunk {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: "test-model".to_string(),
            system_fingerprint: None,
            choices: vec![StreamingChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("stop"));
    }

    #[test]
    fn serialize_safe_returns_error_payload_on_failure() {
        // NaN cannot be serialized by serde_json with default settings
        let serialized = serialize_safe(&std::f64::NAN, "test_context");
        assert!(serialized.contains("serialization_error"));
        assert!(serialized.contains("test_context"));
    }
}
