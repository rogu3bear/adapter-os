//! Unit tests for inference handlers
//!
//! Tests for handler functions, serde serialization/deserialization,
//! and struct default values.

use adapteros_server_api_inference::handlers::{
    inference_handler, inference_health, InferenceRequest, InferenceResponse, UsageStats,
};
use axum::{response::IntoResponse, Json};
use serde_json;

/// Test that inference_health returns expected JSON structure
#[tokio::test]
async fn test_inference_health_returns_ok_status() {
    let response = inference_health().await;
    let response = response.into_response();

    // Extract the JSON body
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read response body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("Body is not valid UTF-8");

    // Parse and verify structure
    let parsed: serde_json::Value =
        serde_json::from_str(&body_str).expect("Failed to parse JSON");

    assert_eq!(parsed.get("status").and_then(|v| v.as_str()), Some("ok"));
    assert_eq!(
        parsed.get("subsystem").and_then(|v| v.as_str()),
        Some("inference")
    );

    // Verify it has exactly the expected keys
    assert_eq!(parsed.as_object().unwrap().len(), 2);
}

/// Test that inference_handler returns a placeholder InferenceResponse
#[tokio::test]
async fn test_inference_handler_returns_placeholder() {
    let request = InferenceRequest {
        prompt: "Test prompt".to_string(),
        model: Some("test-model".to_string()),
        max_tokens: Some(100),
        temperature: Some(0.7),
    };

    let response = inference_handler(Json(request)).await;
    let response = response.into_response();

    // Extract the JSON body
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read response body");
    let body_str = String::from_utf8(body_bytes.to_vec()).expect("Body is not valid UTF-8");

    // Deserialize to InferenceResponse
    let inference_response: InferenceResponse =
        serde_json::from_str(&body_str).expect("Failed to parse InferenceResponse");

    // Verify placeholder values
    assert_eq!(
        inference_response.text,
        "Placeholder response - inference not yet implemented"
    );
    assert_eq!(inference_response.model, "placeholder");
    assert_eq!(inference_response.usage.prompt_tokens, 0);
    assert_eq!(inference_response.usage.completion_tokens, 0);
    assert_eq!(inference_response.usage.total_tokens, 0);
}

/// Test serde serialization of InferenceRequest with all fields
#[test]
fn test_inference_request_serialization_all_fields() {
    let request = InferenceRequest {
        prompt: "Hello, world!".to_string(),
        model: Some("gpt-4".to_string()),
        max_tokens: Some(150),
        temperature: Some(0.8),
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

    assert_eq!(parsed["prompt"], "Hello, world!");
    assert_eq!(parsed["model"], "gpt-4");
    assert_eq!(parsed["max_tokens"], 150);
    assert_eq!(parsed["temperature"], 0.8);
}

/// Test serde serialization of InferenceRequest with optional fields missing
#[test]
fn test_inference_request_serialization_minimal() {
    let request = InferenceRequest {
        prompt: "Hello".to_string(),
        model: None,
        max_tokens: None,
        temperature: None,
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

    assert_eq!(parsed["prompt"], "Hello");
    assert!(parsed["model"].is_null());
    assert!(parsed["max_tokens"].is_null());
    assert!(parsed["temperature"].is_null());
}

/// Test serde deserialization of InferenceRequest
#[test]
fn test_inference_request_deserialization() {
    let json = r#"{
        "prompt": "Test prompt",
        "model": "llama-2-7b",
        "max_tokens": 200,
        "temperature": 0.5
    }"#;

    let request: InferenceRequest = serde_json::from_str(json).expect("Failed to deserialize");

    assert_eq!(request.prompt, "Test prompt");
    assert_eq!(request.model, Some("llama-2-7b".to_string()));
    assert_eq!(request.max_tokens, Some(200));
    assert_eq!(request.temperature, Some(0.5));
}

/// Test serde deserialization of InferenceRequest with missing optional fields
#[test]
fn test_inference_request_deserialization_minimal() {
    let json = r#"{"prompt": "Minimal test"}"#;

    let request: InferenceRequest = serde_json::from_str(json).expect("Failed to deserialize");

    assert_eq!(request.prompt, "Minimal test");
    assert_eq!(request.model, None);
    assert_eq!(request.max_tokens, None);
    assert_eq!(request.temperature, None);
}

/// Test serde serialization of InferenceResponse
#[test]
fn test_inference_response_serialization() {
    let response = InferenceResponse {
        text: "Generated text".to_string(),
        model: "qwen-2.5-7b".to_string(),
        usage: UsageStats {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        },
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

    assert_eq!(parsed["text"], "Generated text");
    assert_eq!(parsed["model"], "qwen-2.5-7b");
    assert_eq!(parsed["usage"]["prompt_tokens"], 10);
    assert_eq!(parsed["usage"]["completion_tokens"], 20);
    assert_eq!(parsed["usage"]["total_tokens"], 30);
}

/// Test serde deserialization of InferenceResponse
#[test]
fn test_inference_response_deserialization() {
    let json = r#"{
        "text": "Deserialized text",
        "model": "test-model",
        "usage": {
            "prompt_tokens": 5,
            "completion_tokens": 15,
            "total_tokens": 20
        }
    }"#;

    let response: InferenceResponse = serde_json::from_str(json).expect("Failed to deserialize");

    assert_eq!(response.text, "Deserialized text");
    assert_eq!(response.model, "test-model");
    assert_eq!(response.usage.prompt_tokens, 5);
    assert_eq!(response.usage.completion_tokens, 15);
    assert_eq!(response.usage.total_tokens, 20);
}

/// Test UsageStats serialization
#[test]
fn test_usage_stats_serialization() {
    let stats = UsageStats {
        prompt_tokens: 100,
        completion_tokens: 200,
        total_tokens: 300,
    };

    let json = serde_json::to_string(&stats).expect("Failed to serialize");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse JSON");

    assert_eq!(parsed["prompt_tokens"], 100);
    assert_eq!(parsed["completion_tokens"], 200);
    assert_eq!(parsed["total_tokens"], 300);
}

/// Test UsageStats deserialization
#[test]
fn test_usage_stats_deserialization() {
    let json = r#"{
        "prompt_tokens": 50,
        "completion_tokens": 75,
        "total_tokens": 125
    }"#;

    let stats: UsageStats = serde_json::from_str(json).expect("Failed to deserialize");

    assert_eq!(stats.prompt_tokens, 50);
    assert_eq!(stats.completion_tokens, 75);
    assert_eq!(stats.total_tokens, 125);
}

/// Test UsageStats with zero values (typical default)
#[test]
fn test_usage_stats_zero_values() {
    let stats = UsageStats {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };

    assert_eq!(stats.prompt_tokens, 0);
    assert_eq!(stats.completion_tokens, 0);
    assert_eq!(stats.total_tokens, 0);

    // Verify serialization round-trip
    let json = serde_json::to_string(&stats).expect("Failed to serialize");
    let deserialized: UsageStats = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.prompt_tokens, 0);
    assert_eq!(deserialized.completion_tokens, 0);
    assert_eq!(deserialized.total_tokens, 0);
}

/// Test InferenceRequest clone trait
#[test]
fn test_inference_request_clone() {
    let original = InferenceRequest {
        prompt: "Clone test".to_string(),
        model: Some("model-1".to_string()),
        max_tokens: Some(50),
        temperature: Some(0.9),
    };

    let cloned = original.clone();

    assert_eq!(cloned.prompt, original.prompt);
    assert_eq!(cloned.model, original.model);
    assert_eq!(cloned.max_tokens, original.max_tokens);
    assert_eq!(cloned.temperature, original.temperature);
}

/// Test InferenceResponse clone trait
#[test]
fn test_inference_response_clone() {
    let original = InferenceResponse {
        text: "Clone test response".to_string(),
        model: "model-2".to_string(),
        usage: UsageStats {
            prompt_tokens: 1,
            completion_tokens: 2,
            total_tokens: 3,
        },
    };

    let cloned = original.clone();

    assert_eq!(cloned.text, original.text);
    assert_eq!(cloned.model, original.model);
    assert_eq!(cloned.usage.prompt_tokens, original.usage.prompt_tokens);
    assert_eq!(
        cloned.usage.completion_tokens,
        original.usage.completion_tokens
    );
    assert_eq!(cloned.usage.total_tokens, original.usage.total_tokens);
}

/// Test UsageStats clone trait
#[test]
fn test_usage_stats_clone() {
    let original = UsageStats {
        prompt_tokens: 10,
        completion_tokens: 20,
        total_tokens: 30,
    };

    let cloned = original.clone();

    assert_eq!(cloned.prompt_tokens, original.prompt_tokens);
    assert_eq!(cloned.completion_tokens, original.completion_tokens);
    assert_eq!(cloned.total_tokens, original.total_tokens);
}

/// Test InferenceRequest debug trait
#[test]
fn test_inference_request_debug() {
    let request = InferenceRequest {
        prompt: "Debug test".to_string(),
        model: Some("debug-model".to_string()),
        max_tokens: Some(10),
        temperature: Some(0.1),
    };

    let debug_str = format!("{:?}", request);
    assert!(debug_str.contains("InferenceRequest"));
    assert!(debug_str.contains("Debug test"));
}

/// Test edge case: empty prompt
#[test]
fn test_inference_request_empty_prompt() {
    let request = InferenceRequest {
        prompt: String::new(),
        model: None,
        max_tokens: None,
        temperature: None,
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    let deserialized: InferenceRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.prompt, "");
}

/// Test edge case: very large max_tokens
#[test]
fn test_inference_request_large_max_tokens() {
    let request = InferenceRequest {
        prompt: "Large tokens test".to_string(),
        model: None,
        max_tokens: Some(u32::MAX),
        temperature: None,
    };

    let json = serde_json::to_string(&request).expect("Failed to serialize");
    let deserialized: InferenceRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.max_tokens, Some(u32::MAX));
}

/// Test edge case: temperature at boundaries
#[test]
fn test_inference_request_temperature_boundaries() {
    let request_zero = InferenceRequest {
        prompt: "Zero temp".to_string(),
        model: None,
        max_tokens: None,
        temperature: Some(0.0),
    };

    let json = serde_json::to_string(&request_zero).expect("Failed to serialize");
    let deserialized: InferenceRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.temperature, Some(0.0));

    let request_high = InferenceRequest {
        prompt: "High temp".to_string(),
        model: None,
        max_tokens: None,
        temperature: Some(2.0),
    };

    let json = serde_json::to_string(&request_high).expect("Failed to serialize");
    let deserialized: InferenceRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.temperature, Some(2.0));
}

/// Test edge case: empty model string
#[test]
fn test_inference_response_empty_model() {
    let response = InferenceResponse {
        text: "Text".to_string(),
        model: String::new(),
        usage: UsageStats {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        },
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    let deserialized: InferenceResponse =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.model, "");
}

/// Test edge case: maximum u32 values in UsageStats
#[test]
fn test_usage_stats_max_values() {
    let stats = UsageStats {
        prompt_tokens: u32::MAX,
        completion_tokens: u32::MAX,
        total_tokens: u32::MAX,
    };

    let json = serde_json::to_string(&stats).expect("Failed to serialize");
    let deserialized: UsageStats = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.prompt_tokens, u32::MAX);
    assert_eq!(deserialized.completion_tokens, u32::MAX);
    assert_eq!(deserialized.total_tokens, u32::MAX);
}
