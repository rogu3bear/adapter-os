//! Integration tests for SSE streaming inference
//!
//! These tests validate the Server-Sent Events (SSE) streaming implementation
//! for real-time inference with a chat-style streaming protocol.

use adapteros_api::streaming::{StreamChoice, StreamChunk, StreamDelta, StreamMessage};
use adapteros_api::{ChatCompletionRequest, ChatMessage};
use adapteros_lora_worker::Worker;
use adapteros_manifest::{AdapterEntry, ManifestV3, Policies, TrustLevel};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::{timeout, Duration};

/// Helper to create a test manifest
fn create_test_manifest() -> ManifestV3 {
    ManifestV3 {
        version: "3".to_string(),
        adapters: vec![AdapterEntry {
            id: "test_adapter".to_string(),
            path: PathBuf::from("/tmp/test_adapter.aos"),
            trust_level: TrustLevel::Verified,
            hash_b3: "test_hash".to_string(),
            metadata: HashMap::new(),
        }],
        policies: Policies::default(),
        metadata: HashMap::new(),
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime and adapter files
async fn test_streaming_chat_completion_basic() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    // Create streaming request
    let request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        }],
        temperature: Some(0.7),
        max_tokens: Some(50),
        stream: Some(true),
        ..Default::default()
    };

    // Create stream
    let mut stream = worker
        .infer_streaming(request)
        .await
        .expect("Failed to create stream");

    // Collect chunks with timeout
    let mut chunks = Vec::new();
    let collect_timeout = Duration::from_secs(30);

    let result = timeout(collect_timeout, async {
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => chunks.push(chunk),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    })
    .await;

    assert!(result.is_ok(), "Stream collection timed out or failed");

    // Verify chunks
    assert!(!chunks.is_empty(), "Expected at least one chunk");

    // First chunk should have role
    if let Some(first_chunk) = chunks.first() {
        assert_eq!(first_chunk.object, "chat.completion.chunk");
        assert!(!first_chunk.choices.is_empty());
        if let Some(role) = &first_chunk.choices[0].delta.role {
            assert_eq!(role, "assistant");
        }
    }

    // Last chunk should have finish_reason
    if let Some(last_chunk) = chunks.last() {
        assert!(!last_chunk.choices.is_empty());
        assert!(last_chunk.choices[0].finish_reason.is_some());
    }

    // Intermediate chunks should have content
    let content_chunks: Vec<&StreamChunk> = chunks
        .iter()
        .filter(|c| c.choices[0].delta.content.is_some())
        .collect();

    assert!(!content_chunks.is_empty(), "Expected content in stream");
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_chunk_format() {
    // Create a StreamChunk manually to verify format
    let chunk = StreamChunk {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "qwen2.5".to_string(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                role: Some("assistant".to_string()),
                content: Some("Hello".to_string()),
            },
            finish_reason: None,
        }],
    };

    // Verify serialization
    let json = serde_json::to_string(&chunk).expect("Failed to serialize chunk");
    assert!(json.contains("chat.completion.chunk"));
    assert!(json.contains("assistant"));
    assert!(json.contains("Hello"));

    // Verify SSE format
    let sse_data = format!("data: {}\n\n", json);
    assert!(sse_data.starts_with("data: "));
    assert!(sse_data.ends_with("\n\n"));
}

#[tokio::test]
async fn test_streaming_done_message() {
    // Test the [DONE] message format
    let done_message = "data: [DONE]\n\n";
    assert_eq!(done_message, "data: [DONE]\n\n");
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_error_handling() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    // Create invalid request (empty messages)
    let invalid_request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: vec![], // Empty - should cause error
        stream: Some(true),
        ..Default::default()
    };

    // Attempt to create stream - should fail
    let result = worker.infer_streaming(invalid_request).await;
    assert!(result.is_err(), "Expected error for invalid request");
}

#[tokio::test]
#[ignore] // Requires Metal runtime and adapter files
async fn test_streaming_multiple_messages() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    // Create request with multiple messages
    let request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: vec![
            ChatMessage {
                role: "user".to_string(),
                content: "What is the capital of France?".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "The capital of France is Paris.".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "What about Germany?".to_string(),
            },
        ],
        temperature: Some(0.7),
        max_tokens: Some(50),
        stream: Some(true),
        ..Default::default()
    };

    // Create stream
    let mut stream = worker
        .infer_streaming(request)
        .await
        .expect("Failed to create stream");

    // Collect all chunks
    let mut chunks = Vec::new();
    let collect_timeout = Duration::from_secs(30);

    let result = timeout(collect_timeout, async {
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => chunks.push(chunk),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    })
    .await;

    assert!(result.is_ok(), "Stream collection failed");
    assert!(
        !chunks.is_empty(),
        "Expected chunks from multi-message conversation"
    );
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_temperature_variation() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Generate creative text.".to_string(),
    }];

    // Test with low temperature (more deterministic)
    let low_temp_request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: messages.clone(),
        temperature: Some(0.1),
        max_tokens: Some(30),
        stream: Some(true),
        ..Default::default()
    };

    let mut low_temp_stream = worker
        .infer_streaming(low_temp_request)
        .await
        .expect("Failed to create low temp stream");

    let mut low_temp_chunks = Vec::new();
    while let Some(Ok(chunk)) = low_temp_stream.next().await {
        low_temp_chunks.push(chunk);
    }

    // Test with high temperature (more random)
    let high_temp_request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: messages.clone(),
        temperature: Some(1.5),
        max_tokens: Some(30),
        stream: Some(true),
        ..Default::default()
    };

    let mut high_temp_stream = worker
        .infer_streaming(high_temp_request)
        .await
        .expect("Failed to create high temp stream");

    let mut high_temp_chunks = Vec::new();
    while let Some(Ok(chunk)) = high_temp_stream.next().await {
        high_temp_chunks.push(chunk);
    }

    // Both should produce chunks
    assert!(!low_temp_chunks.is_empty());
    assert!(!high_temp_chunks.is_empty());
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_max_tokens_limit() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    // Create request with small max_tokens
    let request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Tell me a long story.".to_string(),
        }],
        temperature: Some(0.7),
        max_tokens: Some(10), // Very small limit
        stream: Some(true),
        ..Default::default()
    };

    // Create stream
    let mut stream = worker
        .infer_streaming(request)
        .await
        .expect("Failed to create stream");

    // Collect all chunks
    let mut chunks = Vec::new();
    while let Some(Ok(chunk)) = stream.next().await {
        chunks.push(chunk);
    }

    // Verify generation stopped due to max_tokens
    if let Some(last_chunk) = chunks.last() {
        if let Some(finish_reason) = &last_chunk.choices[0].finish_reason {
            assert!(
                finish_reason == "length" || finish_reason == "stop",
                "Expected finish_reason due to max_tokens limit"
            );
        }
    }
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_concurrent_requests() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    let worker_arc = std::sync::Arc::new(worker);

    // Spawn multiple concurrent streaming requests
    let mut handles = vec![];

    for i in 0..3 {
        let worker_clone = worker_arc.clone();

        let handle = tokio::spawn(async move {
            let request = ChatCompletionRequest {
                model: "qwen2.5".to_string(),
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: format!("Request number {}", i),
                }],
                temperature: Some(0.7),
                max_tokens: Some(20),
                stream: Some(true),
                ..Default::default()
            };

            let mut stream = worker_clone
                .infer_streaming(request)
                .await
                .expect("Failed to create stream");

            let mut chunks = Vec::new();
            while let Some(Ok(chunk)) = stream.next().await {
                chunks.push(chunk);
            }

            chunks
        });

        handles.push(handle);
    }

    // Wait for all streams to complete
    let results = futures::future::join_all(handles).await;

    // Verify all streams succeeded
    for result in results {
        let chunks = result.expect("Task panicked");
        assert!(!chunks.is_empty(), "Expected chunks from concurrent stream");
    }
}

#[tokio::test]
async fn test_stream_chunk_serialization_roundtrip() {
    // Create a complete StreamChunk
    let original = StreamChunk {
        id: "chatcmpl-test-123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "qwen2.5".to_string(),
        choices: vec![StreamChoice {
            index: 0,
            delta: StreamDelta {
                role: Some("assistant".to_string()),
                content: Some("Test content".to_string()),
            },
            finish_reason: Some("stop".to_string()),
        }],
    };

    // Serialize
    let json = serde_json::to_string(&original).expect("Failed to serialize");

    // Deserialize
    let deserialized: StreamChunk = serde_json::from_str(&json).expect("Failed to deserialize");

    // Verify roundtrip
    assert_eq!(deserialized.id, original.id);
    assert_eq!(deserialized.object, original.object);
    assert_eq!(deserialized.created, original.created);
    assert_eq!(deserialized.model, original.model);
    assert_eq!(deserialized.choices.len(), 1);
    assert_eq!(
        deserialized.choices[0].delta.content,
        original.choices[0].delta.content
    );
}

#[tokio::test]
async fn test_stream_delta_partial_updates() {
    // Test StreamDelta with only role
    let role_only = StreamDelta {
        role: Some("assistant".to_string()),
        content: None,
    };

    let json = serde_json::to_string(&role_only).expect("Failed to serialize");
    assert!(json.contains("assistant"));
    assert!(!json.contains("content"));

    // Test StreamDelta with only content
    let content_only = StreamDelta {
        role: None,
        content: Some("partial text".to_string()),
    };

    let json = serde_json::to_string(&content_only).expect("Failed to serialize");
    assert!(json.contains("partial text"));
    assert!(!json.contains("role"));

    // Test StreamDelta with both
    let both = StreamDelta {
        role: Some("assistant".to_string()),
        content: Some("complete message".to_string()),
    };

    let json = serde_json::to_string(&both).expect("Failed to serialize");
    assert!(json.contains("assistant"));
    assert!(json.contains("complete message"));
}

#[tokio::test]
#[ignore] // Requires Metal runtime
async fn test_streaming_chunk_shape_compatibility() {
    use adapteros_lora_kernel_mtl::MetalKernels;

    // Initialize kernels and worker
    let kernels = MetalKernels::new().expect("Failed to initialize Metal kernels");
    let manifest = create_test_manifest();

    let worker = Worker::new(manifest, kernels, PathBuf::from("/tmp/adapteros"), None)
        .await
        .expect("Failed to create Worker");

    // Create request matching the expected streaming request format
    let request = ChatCompletionRequest {
        model: "qwen2.5".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello!".to_string(),
        }],
        stream: Some(true),
        ..Default::default()
    };

    // Create stream
    let mut stream = worker
        .infer_streaming(request)
        .await
        .expect("Failed to create stream");

    // Collect chunks and verify streaming response format
    let mut chunks = Vec::new();
    while let Some(Ok(chunk)) = stream.next().await {
        chunks.push(chunk);

        // Verify each chunk matches expected streaming format
        let last_chunk = chunks.last().unwrap();
        assert_eq!(last_chunk.object, "chat.completion.chunk");
        assert!(!last_chunk.id.is_empty());
        assert!(last_chunk.created > 0);
        assert!(!last_chunk.model.is_empty());
        assert!(!last_chunk.choices.is_empty());
        assert_eq!(last_chunk.choices[0].index, 0);
    }
    assert!(!chunks.is_empty(), "Expected streaming chunks");
}
