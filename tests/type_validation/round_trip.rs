//! Round-trip serialization tests: Rust → JSON → Rust
//!
//! These tests verify that types can be serialized to JSON and
//! deserialized back to Rust without data loss.

use adapteros_api_types::*;
use adapteros_server_api::types::*;
use serde_json::{json, Value};

// Helper macro to test round-trip serialization
macro_rules! test_round_trip {
    ($name:ident, $type_name:ty, $value:expr, $test_fn:expr) => {
        #[tokio::test]
        async fn $name() {
            let original: $type_name = $value;

            // Serialize to JSON
            let json = serde_json::to_value(&original).expect("Failed to serialize to JSON");

            // Deserialize back to Rust
            let deserialized: $type_name =
                serde_json::from_value(json.clone()).expect("Failed to deserialize from JSON");

            // Test function to validate round-trip correctness
            $test_fn(&original, &deserialized, &json);
        }
    };
}

// ============================================================================
// Inference Types
// ============================================================================

#[tokio::test]
async fn test_infer_response_round_trip() {
    let original = InferResponse {
        text: "The quick brown fox".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: InferResponse = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.text, deserialized.text);
    assert_eq!(original.token_count, deserialized.token_count);
    assert_eq!(original.latency_ms, deserialized.latency_ms);
}

#[tokio::test]
async fn test_inference_trace_round_trip() {
    let original = InferenceTrace {
        cpid: "test-cpid-123".to_string(),
        input_tokens: vec![1, 2, 3, 4, 5],
        generated_tokens: vec![6, 7, 8],
        router_decisions: vec![],
        evidence: vec!["Document A".to_string()],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: InferenceTrace = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.cpid, deserialized.cpid);
    assert_eq!(original.input_tokens, deserialized.input_tokens);
    assert_eq!(original.generated_tokens, deserialized.generated_tokens);
    assert_eq!(original.evidence, deserialized.evidence);
}

#[tokio::test]
async fn test_router_decision_round_trip() {
    let original = RouterDecision {
        step: 0,
        selected_adapter: "adapter-a".to_string(),
        confidence: 0.95,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: RouterDecision = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.step, deserialized.step);
    assert_eq!(original.selected_adapter, deserialized.selected_adapter);
    assert!((original.confidence - deserialized.confidence).abs() < 1e-6);
}

#[tokio::test]
async fn test_batch_infer_request_round_trip() {
    let original = BatchInferRequest {
        requests: vec![
            BatchInferItemRequest {
                id: "req-1".to_string(),
                request: InferRequest {
                    prompt: "What is AI?".to_string(),
                    max_tokens: Some(100),
                    temperature: None,
                },
            },
            BatchInferItemRequest {
                id: "req-2".to_string(),
                request: InferRequest {
                    prompt: "Explain ML".to_string(),
                    max_tokens: Some(200),
                    temperature: Some(0.7),
                },
            },
        ],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: BatchInferRequest = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.requests.len(), deserialized.requests.len());
    assert_eq!(original.requests[0].id, deserialized.requests[0].id);
    assert_eq!(
        original.requests[1].request.max_tokens,
        deserialized.requests[1].request.max_tokens
    );
}

#[tokio::test]
async fn test_batch_infer_response_round_trip() {
    let original = BatchInferResponse {
        responses: vec![
            BatchInferItemResponse {
                id: "req-1".to_string(),
                response: Some(InferResponse {
                    text: "AI is...".to_string(),
                    token_count: 10,
                    latency_ms: 100,
                    trace: None,
                }),
                error: None,
            },
            BatchInferItemResponse {
                id: "req-2".to_string(),
                response: None,
                error: Some(ErrorResponse {
                    schema_version: "1.0".to_string(),
                    error: "Model unavailable".to_string(),
                    code: "MODEL_UNAVAILABLE".to_string(),
                    details: None,
                }),
            },
        ],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: BatchInferResponse =
        serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.responses.len(), deserialized.responses.len());
    assert!(deserialized.responses[0].response.is_some());
    assert!(deserialized.responses[1].error.is_some());
}

// ============================================================================
// Adapter Types
// ============================================================================

#[tokio::test]
async fn test_adapter_response_round_trip() {
    let original = AdapterResponse {
        id: "adapter-123".to_string(),
        name: "code-assistant".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: AdapterResponse = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.name, deserialized.name);
    assert_eq!(original.version, deserialized.version);
}

#[tokio::test]
async fn test_adapter_manifest_round_trip() {
    let original = AdapterManifest {
        id: "adapter-123".to_string(),
        name: "code-assistant".to_string(),
        version: "1.0.0".to_string(),
        description: Some("AI code assistant".to_string()),
        author: Some("Team".to_string()),
        hash: Some("abc123".to_string()),
        compatible_models: vec!["qwen-7b".to_string()],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: AdapterManifest = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.description, deserialized.description);
    assert_eq!(original.compatible_models, deserialized.compatible_models);
}

// ============================================================================
// Error Response Types
// ============================================================================

#[tokio::test]
async fn test_error_response_round_trip_minimal() {
    let original = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Not found".to_string(),
        code: "NOT_FOUND".to_string(),
        details: None,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: ErrorResponse = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.error, deserialized.error);
    assert_eq!(original.code, deserialized.code);
    assert_eq!(original.details, deserialized.details);
}

#[tokio::test]
async fn test_error_response_round_trip_with_details() {
    let original = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Validation failed".to_string(),
        code: "BAD_REQUEST".to_string(),
        details: Some(json!({
            "field": "email",
            "reason": "invalid format"
        })),
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: ErrorResponse = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.details, deserialized.details);
}

// ============================================================================
// Health and Status Types
// ============================================================================

#[tokio::test]
async fn test_health_response_round_trip() {
    let original = HealthResponse {
        schema_version: "1.0".to_string(),
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: Some(ModelRuntimeHealth {
            total_models: 5,
            loaded_count: 3,
            healthy: true,
            inconsistencies_count: 0,
        }),
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: HealthResponse = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.status, deserialized.status);
    assert!(deserialized.models.is_some());
    assert_eq!(
        original.models.unwrap().total_models,
        deserialized.models.unwrap().total_models
    );
}

#[tokio::test]
async fn test_model_runtime_health_round_trip() {
    let original = ModelRuntimeHealth {
        total_models: 10,
        loaded_count: 7,
        healthy: true,
        inconsistencies_count: 1,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: ModelRuntimeHealth =
        serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.total_models, deserialized.total_models);
    assert_eq!(original.loaded_count, deserialized.loaded_count);
    assert_eq!(original.healthy, deserialized.healthy);
}

// ============================================================================
// Complex Types with Nested Structures
// ============================================================================

#[tokio::test]
async fn test_routing_decision_round_trip() {
    let original = RoutingDecision {
        id: "decision-123".to_string(),
        tenant_id: "tenant-1".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        request_id: Some("req-123".to_string()),
        step: 0,
        input_token_id: Some(42),
        stack_id: Some("stack-1".to_string()),
        stack_name: Some("production".to_string()),
        stack_hash: Some("hash123".to_string()),
        entropy: 2.5,
        tau: 1.0,
        entropy_floor: 0.1,
        k_value: Some(5),
        candidates: vec![],
        router_latency_us: Some(500),
        total_inference_latency_us: Some(2000),
        overhead_pct: Some(25.0),
        adapters_used: vec!["adapter-1".to_string()],
        activations: vec![0.95],
        reason: "routing complete".to_string(),
        trace_id: "trace-123".to_string(),
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: RoutingDecision = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.id, deserialized.id);
    assert_eq!(original.tenant_id, deserialized.tenant_id);
    assert_eq!(original.step, deserialized.step);
    assert_eq!(original.entropy, deserialized.entropy);
}

#[tokio::test]
async fn test_paginated_response_round_trip() {
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct TestItem {
        id: String,
        name: String,
    }

    let original = PaginatedResponse {
        schema_version: "1.0".to_string(),
        data: vec![
            TestItem {
                id: "1".to_string(),
                name: "Item 1".to_string(),
            },
            TestItem {
                id: "2".to_string(),
                name: "Item 2".to_string(),
            },
        ],
        total: 42,
        page: 1,
        limit: 2,
        pages: 21,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: PaginatedResponse<TestItem> =
        serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.data.len(), deserialized.data.len());
    assert_eq!(original.total, deserialized.total);
    assert_eq!(original.pages, deserialized.pages);
}

// ============================================================================
// Field Name Consistency Tests
// ============================================================================

#[tokio::test]
async fn test_field_names_use_snake_case() {
    let response = InferResponse {
        text: "test".to_string(),
        token_count: 10,
        latency_ms: 100,
        trace: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Validate all keys are snake_case
    if let Some(obj) = json.as_object() {
        for key in obj.keys() {
            // token_count, latency_ms are valid snake_case
            assert!(
                key.chars()
                    .all(|c| c.is_lowercase() || c == '_' || c.is_numeric()),
                "Field '{}' is not in snake_case",
                key
            );
        }
    }
}

#[tokio::test]
async fn test_nested_field_names_snake_case() {
    let original = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![1, 2, 3],
        generated_tokens: vec![4, 5, 6],
        router_decisions: vec![],
        evidence: vec![],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");

    if let Some(obj) = json.as_object() {
        for key in obj.keys() {
            // input_tokens, generated_tokens, router_decisions are valid
            assert!(
                key.chars()
                    .all(|c| c.is_lowercase() || c == '_' || c.is_numeric()),
                "Nested field '{}' violates snake_case convention",
                key
            );
        }
    }
}

// ============================================================================
// Optional Field Handling
// ============================================================================

#[tokio::test]
async fn test_optional_fields_omitted_when_none() {
    let response = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Test error".to_string(),
        code: "TEST_ERROR".to_string(),
        details: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // `details` field should be omitted when None
    if let Some(obj) = json.as_object() {
        assert!(
            !obj.contains_key("details"),
            "None fields should be omitted from JSON"
        );
    }
}

#[tokio::test]
async fn test_optional_fields_included_when_some() {
    let response = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Test error".to_string(),
        code: "TEST_ERROR".to_string(),
        details: Some(json!({"field": "value"})),
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // `details` field should be present when Some
    if let Some(obj) = json.as_object() {
        assert!(
            obj.contains_key("details"),
            "Some fields should be included in JSON"
        );
    }
}

// ============================================================================
// Type Precision Tests
// ============================================================================

#[tokio::test]
async fn test_f64_precision_preserved() {
    let original = RouterDecision {
        id: "test".to_string(),
        tenant_id: "tenant".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        request_id: None,
        step: 0,
        input_token_id: None,
        stack_id: None,
        stack_name: None,
        stack_hash: None,
        entropy: 3.14159265359, // High precision value
        tau: 1.0,
        entropy_floor: 0.001,
        k_value: None,
        candidates: vec![],
        router_latency_us: None,
        total_inference_latency_us: None,
        overhead_pct: None,
        adapters_used: vec![],
        activations: vec![],
        reason: "test".to_string(),
        trace_id: "test".to_string(),
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: RoutingDecision = serde_json::from_value(json).expect("deserialize failed");

    // Check precision is maintained within acceptable bounds
    assert!((original.entropy - deserialized.entropy).abs() < 1e-10);
}

#[tokio::test]
async fn test_integer_type_preservation() {
    let original = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![1, 2, 3, 256, 65535],
        generated_tokens: vec![42, 999],
        router_decisions: vec![],
        evidence: vec![],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: InferenceTrace = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.input_tokens, deserialized.input_tokens);
}

// ============================================================================
// Large Payload Tests
// ============================================================================

#[tokio::test]
async fn test_large_batch_request_round_trip() {
    let requests = (0..100)
        .map(|i| BatchInferItemRequest {
            id: format!("req-{}", i),
            request: InferRequest {
                prompt: format!("Query {}", i),
                max_tokens: Some(100 + i as usize),
                temperature: Some(0.5 + (i as f64 * 0.01)),
            },
        })
        .collect();

    let original = BatchInferRequest { requests };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: BatchInferRequest = serde_json::from_value(json).expect("deserialize failed");

    assert_eq!(original.requests.len(), 100);
    assert_eq!(deserialized.requests.len(), 100);
    assert_eq!(original.requests[50].id, deserialized.requests[50].id);
}
