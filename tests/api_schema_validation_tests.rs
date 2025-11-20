//! Integration tests for API response schema validation

use adapteros_server_api::validation::response_schemas::{
    ResponseSchemaValidator, ResponseValidationMiddleware, SharedResponseValidator,
};
use adapteros_telemetry::TelemetryWriter;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn test_inference_response_validation_success() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let valid_response = json!({
        "text": "The quick brown fox jumps over the lazy dog.",
        "token_count": 42,
        "latency_ms": 150,
        "trace": {
            "cpid": "test-request-12345",
            "input_tokens": [1, 2, 3, 4, 5],
            "generated_tokens": [6, 7, 8, 9, 10, 11, 12],
            "router_decisions": [
                {
                    "step": 0,
                    "selected_adapter": "adapter-a",
                    "confidence": 0.95
                }
            ],
            "evidence": [
                "Document A provides context about foxes",
                "Document B mentions jumping animals"
            ]
        }
    });

    let result = middleware
        .validate_and_handle(&valid_response, "inference_response")
        .await;
    assert!(
        result.is_ok(),
        "Valid inference response should pass validation"
    );
}

#[tokio::test]
async fn test_inference_response_validation_failure_missing_required() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let invalid_response = json!({
        "text": "Incomplete response"
        // Missing token_count, latency_ms
    });

    let result = middleware
        .validate_and_handle(&invalid_response, "inference_response")
        .await;
    assert!(
        result.is_err(),
        "Response missing required fields should fail validation"
    );

    let error = result.unwrap_err();
    match error {
        adapteros_core::AosError::Validation(msg) => {
            assert!(msg.contains("inference_response"));
            assert!(msg.contains("validation failed"));
        }
        _ => panic!("Expected validation error, got {:?}", error),
    }
}

#[tokio::test]
async fn test_inference_response_validation_failure_wrong_types() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let invalid_response = json!({
        "text": 12345,  // Should be string
        "token_count": "not_a_number",  // Should be integer
        "latency_ms": true  // Should be integer
    });

    let result = middleware
        .validate_and_handle(&invalid_response, "inference_response")
        .await;
    assert!(
        result.is_err(),
        "Response with wrong field types should fail validation"
    );
}

#[tokio::test]
async fn test_model_list_response_validation() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let valid_response = json!({
        "models": [
            {
                "id": "qwen2.5-7b-instruct",
                "name": "Qwen2.5 7B Instruct",
                "status": "loaded",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            },
            {
                "id": "llama-3-8b-chat",
                "name": "Llama 3 8B Chat",
                "status": "unloaded"
            }
        ]
    });

    let result = middleware
        .validate_and_handle(&valid_response, "model_list_response")
        .await;
    assert!(
        result.is_ok(),
        "Valid model list response should pass validation"
    );
}

#[tokio::test]
async fn test_error_response_validation() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let valid_error_response = json!({
        "error": {
            "message": "Model not found",
            "code": "MODEL_NOT_FOUND",
            "details": {
                "model_id": "unknown-model",
                "available_models": ["qwen2.5-7b", "llama-3-8b"]
            }
        }
    });

    let result = middleware
        .validate_and_handle(&valid_error_response, "error_response")
        .await;
    assert!(
        result.is_ok(),
        "Valid error response should pass validation"
    );
}

#[tokio::test]
async fn test_monitor_only_mode() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let invalid_response = json!({
        "text": "Missing fields"
    });

    let result = middleware
        .validate_monitor_only(&invalid_response, "inference_response")
        .await;

    assert!(
        !result.valid,
        "Invalid response should be detected in monitor mode"
    );
    assert!(!result.errors.is_empty(), "Should have validation errors");
    assert_eq!(result.schema_name, "inference_response");
}

#[tokio::test]
async fn test_unknown_schema_handling() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let response = json!({"test": "data"});

    let result = middleware
        .validate_and_handle(&response, "nonexistent_schema")
        .await;
    assert!(
        result.is_err(),
        "Unknown schema should cause validation error"
    );
}

#[tokio::test]
async fn test_schema_registration_and_usage() {
    let mut base_validator = ResponseSchemaValidator::new(None);

    // Register a custom schema
    let custom_schema = adapteros_server_api::validation::response_schemas::ResponseSchema {
        name: "custom_response".to_string(),
        schema: json!({
            "type": "object",
            "required": ["status", "data"],
            "properties": {
                "status": {"type": "string"},
                "data": {"type": "object"}
            }
        }),
        required: true,
        version: "1.0.0".to_string(),
    };

    assert!(base_validator.register_schema(custom_schema).is_ok());
    assert!(base_validator.has_schema("custom_response"));

    let validator = Arc::new(base_validator);
    let middleware = ResponseValidationMiddleware::new(validator);

    // Test valid custom response
    let valid_response = json!({
        "status": "success",
        "data": {
            "items": [1, 2, 3],
            "count": 3
        }
    });

    let result = middleware
        .validate_and_handle(&valid_response, "custom_response")
        .await;
    assert!(
        result.is_ok(),
        "Valid custom response should pass validation"
    );

    // Test invalid custom response
    let invalid_response = json!({
        "data": {"count": 5}
        // Missing required "status" field
    });

    let result = middleware
        .validate_and_handle(&invalid_response, "custom_response")
        .await;
    assert!(
        result.is_err(),
        "Invalid custom response should fail validation"
    );
}

#[tokio::test]
async fn test_validation_performance() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    // Create a reasonably complex response
    let complex_response = json!({
        "text": "This is a test response with some content that might be generated by an AI model. It contains multiple sentences and should be long enough to test validation performance.",
        "token_count": 156,
        "latency_ms": 2340,
        "trace": {
            "cpid": "perf-test-abcdef123456",
            "input_tokens": (0..50).collect::<Vec<_>>(),
            "generated_tokens": (50..106).collect::<Vec<_>>(),
            "router_decisions": (0..10).map(|i| json!({
                "step": i,
                "selected_adapter": format!("adapter-{}", i),
                "confidence": 0.8 + (i as f64 * 0.01)
            })).collect::<Vec<_>>(),
            "evidence": (0..5).map(|i| format!("Document {} provides relevant context", i)).collect::<Vec<_>>()
        }
    });

    // Run multiple validations to check performance
    let start_time = std::time::Instant::now();

    for _ in 0..100 {
        let result = middleware
            .validate_monitor_only(&complex_response, "inference_response")
            .await;
        assert!(result.valid, "Complex response should be valid");
        assert!(
            result.validation_time_us < 10000,
            "Validation should be fast (< 10ms)"
        );
    }

    let total_time = start_time.elapsed();
    let avg_time_per_validation = total_time.as_micros() as f64 / 100.0;

    assert!(
        avg_time_per_validation < 5000.0,
        "Average validation time should be reasonable (< 5ms): {:.2}μs",
        avg_time_per_validation
    );
}

#[tokio::test]
async fn test_validation_error_details() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(validator);

    let invalid_response = json!({
        "text": 123,  // Wrong type
        "token_count": "not_a_number",  // Wrong type
        // Missing latency_ms
        "extra_field": "should be ignored"
    });

    let result = middleware
        .validate_monitor_only(&invalid_response, "inference_response")
        .await;

    assert!(!result.valid);
    assert!(
        result.errors.len() >= 2,
        "Should have multiple validation errors"
    );

    // Check that error messages are descriptive
    let error_text = result.errors.join(" ");
    assert!(
        error_text.contains("text")
            || error_text.contains("token_count")
            || error_text.contains("latency_ms"),
        "Error messages should mention the problematic fields: {}",
        error_text
    );
}

#[tokio::test]
async fn test_concurrent_validation() {
    let validator = Arc::new(ResponseSchemaValidator::new(None));
    let middleware = ResponseValidationMiddleware::new(Arc::clone(&validator));

    let responses = vec![
        json!({"text": "Response 1", "token_count": 10, "latency_ms": 100}),
        json!({"text": "Response 2", "token_count": 20, "latency_ms": 200}),
        json!({"text": "Response 3", "token_count": 30, "latency_ms": 300}),
        json!({"text": "Response 4", "token_count": 40, "latency_ms": 400}),
    ];

    // Spawn concurrent validation tasks
    let mut handles = vec![];

    for (i, response) in responses.into_iter().enumerate() {
        let middleware_clone = ResponseValidationMiddleware::new(Arc::clone(&validator));

        let handle = tokio::spawn(async move {
            let result = middleware_clone
                .validate_and_handle(&response, "inference_response")
                .await;
            (i, result)
        });

        handles.push(handle);
    }

    // Wait for all validations to complete
    for handle in handles {
        let (index, result) = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "Concurrent validation {} should succeed",
            index
        );
    }
}
