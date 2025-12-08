//! Server-Sent Events (SSE) streaming for real-time inference
//!
//! Provides token-by-token streaming responses for chat completions and text generation
//! using a chat-style chunked response format.

use adapteros_core::Result;
use adapteros_deterministic_exec::spawn_deterministic;
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_lora_worker::StrictnessControl;
use axum::{
    extract::State,
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::{debug, info, warn};

use crate::{ApiError, ApiState};

/// Streaming inference request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingInferenceRequest {
    /// Input prompt or messages
    pub prompt: String,
    /// Model identifier
    #[serde(default)]
    pub model: Option<String>,
    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Temperature for sampling
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Top-p sampling parameter
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Stop sequences
    #[serde(default)]
    pub stop: Vec<String>,
    /// Whether to stream the response
    #[serde(default = "default_stream")]
    pub stream: bool,
    /// Active adapter stack name
    #[serde(default)]
    pub adapter_stack: Option<String>,
    /// Stack ID for telemetry correlation
    #[serde(default)]
    pub stack_id: Option<String>,
    /// Stack version for telemetry correlation
    #[serde(default)]
    pub stack_version: Option<i64>,
}

fn default_max_tokens() -> usize {
    512
}

fn default_temperature() -> f32 {
    0.7
}

fn default_stream() -> bool {
    true
}

/// Streaming response chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChunk {
    /// Unique identifier for the completion
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    pub object: String,
    /// Unix timestamp of when the chunk was created
    pub created: u64,
    /// Model used for generation
    pub model: String,
    /// System fingerprint (determinism tracking)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    /// Array of choices
    pub choices: Vec<StreamingChoice>,
}

/// Individual choice in a streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChoice {
    /// Index of this choice
    pub index: usize,
    /// Delta containing the new content
    pub delta: Delta,
    /// Finish reason (null until complete)
    pub finish_reason: Option<String>,
}

/// Delta containing new content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Content delta (new tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Error response for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorResponse {
    pub error: StreamErrorDetail,
}

/// Error detail for streaming errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Streaming event type
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Token generated
    Token(String),
    /// Generation complete
    Done { finish_reason: String },
    /// Error occurred
    Error(String),
}

/// SSE streaming inference handler
pub async fn streaming_inference_handler<
    K: FusedKernels + StrictnessControl + Send + Sync + 'static,
>(
    State(state): State<Arc<ApiState<K>>>,
    Json(request): Json<StreamingInferenceRequest>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    info!(
        "Starting streaming inference: prompt_len={}, max_tokens={}",
        request.prompt.len(),
        request.max_tokens
    );

    // Generate unique request ID
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let model_name = request
        .model
        .clone()
        .unwrap_or_else(|| "adapteros".to_string());

    // Create channel for streaming events
    let (tx, rx) = mpsc::channel::<StreamEvent>(100);

    // Clone request for background task
    let req = request.clone();
    let state_clone = state.clone();

    // Spawn background generation task
    let spawn_name = format!("streaming-inference-{}", request_id);
    let tx_for_spawn = tx.clone();
    if let Err(e) = spawn_deterministic(spawn_name, async move {
        if let Err(e) = generate_streaming_response(state_clone, req, tx_for_spawn.clone()).await {
            warn!("Streaming generation error: {}", e);
            let _ = tx_for_spawn.send(StreamEvent::Error(e.to_string())).await;
        }
    }) {
        warn!("Failed to spawn deterministic streaming task: {}", e);
        let _ = tx
            .send(StreamEvent::Error("spawn failed".to_string()))
            .await;
    }

    // Convert channel to SSE stream
    let stream = ReceiverStream::new(rx).map(move |event| {
        let chunk = match event {
            StreamEvent::Token(content) => {
                debug!("Streaming token: {}", content);
                StreamingChunk {
                    id: request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    model: model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: Some(content),
                        },
                        finish_reason: None,
                    }],
                }
            }
            StreamEvent::Done { finish_reason } => {
                info!("Streaming complete: {}", finish_reason);
                StreamingChunk {
                    id: request_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    created: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    model: model_name.clone(),
                    system_fingerprint: None,
                    choices: vec![StreamingChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: None,
                        },
                        finish_reason: Some(finish_reason),
                    }],
                }
            }
            StreamEvent::Error(error) => {
                warn!("Streaming error event: {}", error);
                // Send structured streaming error
                let error_response = StreamErrorResponse {
                    error: StreamErrorDetail {
                        message: error.clone(),
                        error_type: "inference_error".to_string(),
                        code: None,
                    },
                };
                let json = serde_json::to_string(&error_response).unwrap_or_else(|_| {
                    format!(
                        r#"{{"error":{{"message":"{}","type":"inference_error"}}}}"#,
                        error
                    )
                });
                return Ok::<_, Infallible>(Event::default().data(json));
            }
        };

        // Serialize chunk to JSON
        let json = serde_json::to_string(&chunk)
            .unwrap_or_else(|e| format!(r#"{{"error":"serialization failed: {}"}}"#, e));

        Ok::<_, Infallible>(Event::default().data(json))
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Generate streaming response from worker
///
/// **IMPORTANT: Streaming is currently simulated.**
///
/// Since Worker.infer() returns complete text, we simulate streaming by
/// chunking the response word-by-word. This provides an SSE interface for
/// clients while the backend generates the full response first.
///
/// **Limitations:**
/// - No time-to-first-token improvement (full generation happens before streaming)
/// - Client disconnect not detected until after generation completes
/// - Resource waste if client disconnects during generation
///
/// **Future improvement:** Implement true token-by-token streaming at the kernel level.
async fn generate_streaming_response<
    K: FusedKernels + StrictnessControl + Send + Sync + 'static,
>(
    state: Arc<ApiState<K>>,
    request: StreamingInferenceRequest,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    // Lock worker and call infer
    let mut worker = state.worker.lock().await;

    // Create Worker InferenceRequest with proper fields
    let inference_req = adapteros_lora_worker::InferenceRequest {
        cpid: uuid::Uuid::new_v4().to_string(),
        prompt: request.prompt.clone(),
        max_tokens: request.max_tokens,
        require_evidence: false,
        request_type: adapteros_lora_worker::RequestType::Normal,
        stack_id: request.stack_id.clone(),
        stack_version: request.stack_version,
        domain_hint: None,
        temperature: None,
        top_k: None,
        top_p: None,
        seed: None,
        router_seed: None,
        seed_mode: None,
        request_seed: None,
        backend_profile: None,
        pinned_adapter_ids: None,
        determinism_mode: "strict".to_string(),
        routing_determinism_mode: None,
        strict_mode: false,
        adapter_strength_overrides: None,
        effective_adapter_ids: None,
        placement: None,
        routing_policy: None,
    };

    debug!(
        "Running inference: prompt_len={}, max_tokens={}",
        request.prompt.len(),
        request.max_tokens
    );

    // Run inference to get complete response
    let response = worker
        .infer(inference_req)
        .await
        .map_err(|e| adapteros_core::AosError::Worker(format!("Inference failed: {}", e)))?;

    // Check for refusal or missing text
    let text = if let Some(text) = response.text {
        text
    } else if let Some(refusal) = response.refusal {
        let error_msg = format!("Request refused: {}", refusal.message);
        tx.send(StreamEvent::Error(error_msg)).await.ok();
        return Ok(());
    } else {
        tx.send(StreamEvent::Error("No text generated".to_string()))
            .await
            .ok();
        return Ok(());
    };
    debug!("Generated text: {} chars", text.len());

    // Simulate streaming by chunking the response
    // Split by words for more natural streaming
    let words: Vec<&str> = text.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        // Add space before word (except first)
        let chunk = if i == 0 {
            word.to_string()
        } else {
            format!(" {}", word)
        };

        // Send chunk to stream
        if tx.send(StreamEvent::Token(chunk)).await.is_err() {
            // Client disconnected
            debug!("Client disconnected during streaming");
            return Ok(());
        }

        // Small delay to simulate progressive generation (optional)
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Send completion event
    tx.send(StreamEvent::Done {
        finish_reason: "stop".to_string(),
    })
    .await
    .ok();

    Ok(())
}

/// Non-streaming completion handler (for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Non-streaming inference handler (returns complete response)
pub async fn completion_handler<K: FusedKernels + StrictnessControl + Send + Sync + 'static>(
    State(state): State<Arc<ApiState<K>>>,
    Json(request): Json<StreamingInferenceRequest>,
) -> std::result::Result<Json<CompletionResponse>, ApiError> {
    info!(
        "Non-streaming completion: prompt_len={}, max_tokens={}",
        request.prompt.len(),
        request.max_tokens
    );

    // Lock worker
    let mut worker = state.worker.lock().await;

    // Create Worker InferenceRequest
    let inference_req = adapteros_lora_worker::InferenceRequest {
        cpid: uuid::Uuid::new_v4().to_string(),
        prompt: request.prompt.clone(),
        max_tokens: request.max_tokens,
        require_evidence: false,
        request_type: adapteros_lora_worker::RequestType::Normal,
        stack_id: request.stack_id.clone(),
        stack_version: request.stack_version,
        domain_hint: None,
        temperature: None,
        top_k: None,
        top_p: None,
        seed: None,
        router_seed: None,
        seed_mode: None,
        request_seed: None,
        backend_profile: None,
        pinned_adapter_ids: None,
        determinism_mode: "strict".to_string(),
        routing_determinism_mode: None,
        strict_mode: false,
        adapter_strength_overrides: None,
        effective_adapter_ids: None,
        placement: None,
        routing_policy: None,
    };

    // Run inference
    let worker_response = worker.infer(inference_req).await?;

    // Extract text or handle refusal
    let output_text = if let Some(text) = worker_response.text {
        text
    } else if let Some(refusal) = worker_response.refusal {
        return Err(ApiError::WorkerError(format!(
            "Request refused: {}",
            refusal.message
        )));
    } else {
        return Err(ApiError::WorkerError("No text generated".to_string()));
    };

    // Build streaming response
    // Note: We don't have exact token counts from Worker, use estimates
    let prompt_token_count = request.prompt.split_whitespace().count();
    let completion_token_count = output_text.split_whitespace().count();

    let response = CompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        model: request.model.unwrap_or_else(|| "adapteros".to_string()),
        choices: vec![CompletionChoice {
            index: 0,
            message: Message {
                role: "assistant".to_string(),
                content: output_text,
            },
            finish_reason: "stop".to_string(),
        }],
        usage: Usage {
            prompt_tokens: prompt_token_count,
            completion_tokens: completion_token_count,
            total_tokens: prompt_token_count + completion_token_count,
        },
    };

    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_request_defaults() {
        let req = StreamingInferenceRequest {
            prompt: "Hello".to_string(),
            model: None,
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: None,
            stop: vec![],
            stream: default_stream(),
            adapter_stack: None,
            stack_id: None,
            stack_version: None,
        };

        assert_eq!(req.max_tokens, 512);
        assert!((req.temperature - 0.7).abs() < 0.01);
        assert!(req.stream);
    }

    #[test]
    fn test_streaming_chunk_serialization() {
        let chunk = StreamingChunk {
            id: "test-123".to_string(),
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

        let json = serde_json::to_string(&chunk).expect("Failed to serialize test chunk");
        assert!(json.contains("Hello"));
        assert!(json.contains("chat.completion.chunk"));
    }
}
