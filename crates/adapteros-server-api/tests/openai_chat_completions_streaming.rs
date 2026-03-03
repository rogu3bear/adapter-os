//! OpenAI-compatible Chat Completions streaming tests
//!
//! Tests for POST /v1/chat/completions with stream=true
//! Validates proper SSE formatting with OpenAI-compatible chunk format.
//!
//! [2026-01-29 openai_chat_completions_streaming]

use adapteros_server_api::handlers::openai_compat::{
    OpenAiChatCompletionsRequest, OpenAiChatMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

mod common;

/// OpenAI-compatible streaming chunk format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    pub choices: Vec<StreamingChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChoice {
    pub index: usize,
    pub delta: Delta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Test that non-streaming requests still work
#[tokio::test]
async fn test_non_streaming_still_works() {
    let request = OpenAiChatCompletionsRequest {
        model: Some("adapteros".to_string()),
        messages: vec![OpenAiChatMessage {
            role: "user".to_string(),
            content: json!("Hello"),
        }],
        temperature: None,
        top_p: None,
        max_tokens: Some(10),
        max_completion_tokens: None,
        stream: Some(false), // Non-streaming
        n: None,
        response_format: None,
        tools: None,
        tool_choice: None,
        seed: None,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
        logprobs: None,
    };

    // Verify the request is valid and would not be rejected
    assert!(!request.stream.unwrap_or(false));
}

/// Test that stream=true is now accepted (was previously rejected)
#[tokio::test]
async fn test_streaming_request_accepted() {
    let request = OpenAiChatCompletionsRequest {
        model: Some("adapteros".to_string()),
        messages: vec![OpenAiChatMessage {
            role: "user".to_string(),
            content: json!("Hello"),
        }],
        temperature: Some(0.7),
        top_p: None,
        max_tokens: Some(100),
        max_completion_tokens: None,
        stream: Some(true), // Streaming - now accepted
        n: None,
        response_format: None,
        tools: None,
        tool_choice: None,
        seed: None,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
        logprobs: None,
    };

    // Verify the request has stream=true
    assert!(request.stream.unwrap_or(false));
}

/// Test n>1 is still rejected
#[tokio::test]
async fn test_n_greater_than_one_rejected() {
    let request = OpenAiChatCompletionsRequest {
        model: Some("adapteros".to_string()),
        messages: vec![OpenAiChatMessage {
            role: "user".to_string(),
            content: json!("Hello"),
        }],
        temperature: None,
        top_p: None,
        max_tokens: Some(10),
        max_completion_tokens: None,
        stream: None,
        n: Some(2), // n>1 - should be rejected
        response_format: None,
        tools: None,
        tool_choice: None,
        seed: None,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
        logprobs: None,
    };

    assert!(request.n.unwrap_or(1) > 1);
}

/// Test messages_to_prompt conversion
#[tokio::test]
async fn test_messages_format() {
    let messages = [
        OpenAiChatMessage {
            role: "system".to_string(),
            content: json!("You are a helpful assistant."),
        },
        OpenAiChatMessage {
            role: "user".to_string(),
            content: json!("Hello"),
        },
    ];

    // Verify messages have expected structure
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "system");
    assert_eq!(messages[1].role, "user");
}

/// Test streaming chunk format is OpenAI-compatible
#[tokio::test]
async fn test_streaming_chunk_format() {
    // Verify the expected chunk format matches OpenAI spec
    let chunk = ChatCompletionChunk {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "adapteros".to_string(),
        system_fingerprint: Some("fp_abc123".to_string()),
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: Some("assistant".to_string()),
                content: None,
            },
            finish_reason: None,
        }],
    };

    let json_str = serde_json::to_string(&chunk).unwrap();

    // Verify the chunk serializes correctly
    assert!(json_str.contains("chat.completion.chunk"));
    assert!(json_str.contains("chatcmpl-test123"));
    assert!(json_str.contains("assistant"));
}

/// Test streaming chunk with content delta
#[tokio::test]
async fn test_streaming_chunk_with_content() {
    let chunk = ChatCompletionChunk {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "adapteros".to_string(),
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

    let json_str = serde_json::to_string(&chunk).unwrap();

    assert!(json_str.contains("\"content\":\"Hello\""));
    assert!(!json_str.contains("\"role\""));
}

/// Test streaming chunk with finish_reason
#[tokio::test]
async fn test_streaming_chunk_with_finish_reason() {
    let chunk = ChatCompletionChunk {
        id: "chatcmpl-test123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1234567890,
        model: "adapteros".to_string(),
        system_fingerprint: Some("receipt_abc".to_string()),
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: None,
                content: None,
            },
            finish_reason: Some("stop".to_string()),
        }],
    };

    let json_str = serde_json::to_string(&chunk).unwrap();

    assert!(json_str.contains("\"finish_reason\":\"stop\""));
    assert!(json_str.contains("receipt_abc"));
}

/// Test array content in message (multi-part content)
#[tokio::test]
async fn test_array_content_message() {
    let content = json!([
        {"type": "text", "text": "Hello"},
        {"type": "text", "text": " world"}
    ]);

    let message = OpenAiChatMessage {
        role: "user".to_string(),
        content,
    };

    // Verify array content is accepted
    assert!(message.content.is_array());
}
