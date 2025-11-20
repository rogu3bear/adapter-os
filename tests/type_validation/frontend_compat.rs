//! Frontend type compatibility tests
//!
//! Validates that API types serialize to JSON that matches TypeScript interfaces
//! in the frontend, particularly around field naming conventions (snake_case)
//! and type compatibility.

use adapteros_api_types::*;
use adapteros_server_api::types::*;
use serde_json::{json, Value};
use std::collections::HashSet;

// ============================================================================
// Field Naming Convention Tests
// ============================================================================

/// Check that a JSON value only uses snake_case field names
fn validate_all_fields_snake_case(value: &Value, path: &str) -> Vec<String> {
    let mut violations = Vec::new();

    match value {
        Value::Object(map) => {
            for (key, val) in map {
                if !is_snake_case(key) {
                    violations.push(format!(
                        "{}.{} is not snake_case (found: {})",
                        path, key, key
                    ));
                }

                // Recursively check nested objects
                let new_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                violations.extend(validate_all_fields_snake_case(val, &new_path));
            }
        }
        Value::Array(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                let new_path = format!("{}[{}]", path, idx);
                violations.extend(validate_all_fields_snake_case(item, &new_path));
            }
        }
        _ => {}
    }

    violations
}

fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with lowercase or underscore
    let first = s.chars().next().unwrap();
    if !first.is_lowercase() && first != '_' {
        return false;
    }

    // Can only contain lowercase, digits, underscores
    s.chars()
        .all(|c| c.is_lowercase() || c.is_numeric() || c == '_')
}

#[tokio::test]
async fn test_infer_response_field_names_match_typescript() {
    // TypeScript expects: text, token_count, latency_ms, trace
    let response = InferResponse {
        text: "Hello, world!".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    let violations = validate_all_fields_snake_case(&json, "InferResponse");
    assert!(
        violations.is_empty(),
        "Field naming violations: {:?}",
        violations
    );
}

#[tokio::test]
async fn test_batch_infer_request_field_names() {
    // TypeScript expects: id, request (flattened with inline request fields)
    let batch = BatchInferRequest {
        requests: vec![BatchInferItemRequest {
            id: "req-1".to_string(),
            request: InferRequest {
                prompt: "test".to_string(),
                max_tokens: Some(100),
                temperature: None,
            },
        }],
    };

    let json = serde_json::to_value(&batch).expect("serialize failed");

    let violations = validate_all_fields_snake_case(&json, "BatchInferRequest");
    assert!(violations.is_empty(), "Violations: {:?}", violations);
}

#[tokio::test]
async fn test_routing_decision_complex_field_names() {
    // Complex nested structure with multiple levels
    let decision = RoutingDecision {
        id: "decision-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        request_id: Some("req-1".to_string()),
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
        trace_id: "trace-1".to_string(),
    };

    let json = serde_json::to_value(&decision).expect("serialize failed");

    let violations = validate_all_fields_snake_case(&json, "RoutingDecision");
    assert!(violations.is_empty(), "Violations: {:?}", violations);
}

#[tokio::test]
async fn test_all_api_response_types_use_snake_case() {
    // Test multiple response types to ensure consistency
    let responses = vec![
        (
            "HealthResponse",
            serde_json::to_value(&HealthResponse {
                schema_version: "1.0".to_string(),
                status: "healthy".to_string(),
                version: "1.0.0".to_string(),
                models: None,
            })
            .unwrap(),
        ),
        (
            "ErrorResponse",
            serde_json::to_value(&ErrorResponse {
                schema_version: "1.0".to_string(),
                error: "Test error".to_string(),
                code: "TEST_ERROR".to_string(),
                details: None,
            })
            .unwrap(),
        ),
        (
            "AdapterResponse",
            serde_json::to_value(&AdapterResponse {
                id: "adapter-1".to_string(),
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                status: "loaded".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-02T00:00:00Z".to_string(),
            })
            .unwrap(),
        ),
    ];

    for (type_name, json) in responses {
        let violations = validate_all_fields_snake_case(&json, type_name);
        assert!(
            violations.is_empty(),
            "{} has violations: {:?}",
            type_name,
            violations
        );
    }
}

// ============================================================================
// Type Compatibility Tests
// ============================================================================

#[tokio::test]
async fn test_string_fields_are_strings() {
    let response = AdapterResponse {
        id: "adapter-123".to_string(),
        name: "test-adapter".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Verify string fields are actually strings
    assert!(json.get("id").unwrap().is_string(), "id should be string");
    assert!(
        json.get("name").unwrap().is_string(),
        "name should be string"
    );
    assert!(
        json.get("version").unwrap().is_string(),
        "version should be string"
    );
    assert!(
        json.get("status").unwrap().is_string(),
        "status should be string"
    );
}

#[tokio::test]
async fn test_numeric_fields_are_numbers() {
    let response = InferResponse {
        text: "test".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Verify numeric fields are numbers, not strings
    assert!(
        json.get("token_count").unwrap().is_number(),
        "token_count should be number, not string"
    );
    assert!(
        json.get("latency_ms").unwrap().is_number(),
        "latency_ms should be number, not string"
    );
}

#[tokio::test]
async fn test_optional_fields_null_handling() {
    // When optional fields are None, they should be omitted or null
    let response = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Test error".to_string(),
        code: "TEST_ERROR".to_string(),
        details: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // With skip_serializing_if = Option::is_none, field should be omitted
    assert!(
        !json.as_object().unwrap().contains_key("details"),
        "None fields should be omitted (skip_serializing_if)"
    );
}

#[tokio::test]
async fn test_optional_fields_some_handling() {
    // When optional fields are Some, they should be present
    let response = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Test error".to_string(),
        code: "TEST_ERROR".to_string(),
        details: Some(json!({"field": "value"})),
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // With Some value, field should be present
    assert!(
        json.as_object().unwrap().contains_key("details"),
        "Some fields should be present"
    );
    assert!(json.get("details").unwrap().is_object());
}

#[tokio::test]
async fn test_array_fields_are_arrays() {
    let trace = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![1, 2, 3],
        generated_tokens: vec![4, 5],
        router_decisions: vec![],
        evidence: vec!["doc1".to_string()],
    };

    let json = serde_json::to_value(&trace).expect("serialize failed");

    // Verify array fields are arrays
    assert!(json.get("input_tokens").unwrap().is_array());
    assert!(json.get("generated_tokens").unwrap().is_array());
    assert!(json.get("router_decisions").unwrap().is_array());
    assert!(json.get("evidence").unwrap().is_array());
}

// ============================================================================
// Frontend Type Mapping Tests
// ============================================================================

#[tokio::test]
async fn test_boolean_field_serialization() {
    let health = HealthResponse {
        schema_version: "1.0".to_string(),
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: Some(ModelRuntimeHealth {
            total_models: 5,
            loaded_count: 3,
            healthy: true, // Boolean field
            inconsistencies_count: 0,
        }),
    };

    let json = serde_json::to_value(&health).expect("serialize failed");

    // Verify boolean field is actually boolean
    let models = json.get("models").unwrap();
    assert!(
        models.get("healthy").unwrap().is_boolean(),
        "healthy should be boolean"
    );

    assert_eq!(
        models.get("healthy").unwrap().as_bool(),
        Some(true),
        "healthy should be true"
    );
}

#[tokio::test]
async fn test_integer_field_serialization() {
    let health = HealthResponse {
        schema_version: "1.0".to_string(),
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: Some(ModelRuntimeHealth {
            total_models: 42,
            loaded_count: 38,
            healthy: true,
            inconsistencies_count: 1,
        }),
    };

    let json = serde_json::to_value(&health).expect("serialize failed");

    let models = json.get("models").unwrap();
    assert!(
        models.get("total_models").unwrap().is_number(),
        "total_models should be number"
    );
    assert_eq!(
        models.get("total_models").unwrap().as_i64(),
        Some(42),
        "total_models value should match"
    );
}

#[tokio::test]
async fn test_float_field_serialization() {
    let decision = RoutingDecision {
        id: "test".to_string(),
        tenant_id: "tenant".to_string(),
        timestamp: "2024-01-01T00:00:00Z".to_string(),
        request_id: None,
        step: 0,
        input_token_id: None,
        stack_id: None,
        stack_name: None,
        stack_hash: None,
        entropy: 2.71828,
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

    let json = serde_json::to_value(&decision).expect("serialize failed");

    // Verify float fields
    assert!(json.get("entropy").unwrap().is_f64());
    assert!(json.get("tau").unwrap().is_f64());
    assert!(json.get("entropy_floor").unwrap().is_f64());
}

// ============================================================================
// Field Presence and Absence Tests
// ============================================================================

#[tokio::test]
async fn test_required_fields_always_present() {
    let adapter = AdapterResponse {
        id: "adapter-1".to_string(),
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&adapter).expect("serialize failed");
    let obj = json.as_object().unwrap();

    // All fields should be present
    assert!(obj.contains_key("id"));
    assert!(obj.contains_key("name"));
    assert!(obj.contains_key("version"));
    assert!(obj.contains_key("status"));
    assert!(obj.contains_key("created_at"));
    assert!(obj.contains_key("updated_at"));
}

#[tokio::test]
async fn test_optional_fields_consistency() {
    // Create two responses: one with optional field, one without
    let with_optional = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Error".to_string(),
        code: "ERROR".to_string(),
        details: Some(json!({"key": "value"})),
    };

    let without_optional = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Error".to_string(),
        code: "ERROR".to_string(),
        details: None,
    };

    let json_with = serde_json::to_value(&with_optional).expect("serialize failed");
    let json_without = serde_json::to_value(&without_optional).expect("serialize failed");

    // With optional
    assert!(json_with.get("details").is_some());
    assert!(json_with.get("details").unwrap().is_object());

    // Without optional (omitted)
    assert!(json_without.get("details").is_none());
}

// ============================================================================
// Frontend API Client Compatibility
// ============================================================================

#[tokio::test]
async fn test_pagination_params_frontend_compatibility() {
    // Frontend expects page and limit as numbers
    let params = PaginationParams { page: 2, limit: 25 };

    let json = serde_json::to_value(&params).expect("serialize failed");

    // Verify types match TypeScript interface expectations
    assert!(json.get("page").unwrap().is_number());
    assert!(json.get("limit").unwrap().is_number());
    assert_eq!(json.get("page").unwrap().as_u64(), Some(2));
    assert_eq!(json.get("limit").unwrap().as_u64(), Some(25));
}

#[tokio::test]
async fn test_health_response_frontend_structure() {
    // Frontend expects specific structure for health check
    let health = HealthResponse {
        schema_version: "1.0".to_string(),
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: Some(ModelRuntimeHealth {
            total_models: 10,
            loaded_count: 8,
            healthy: true,
            inconsistencies_count: 0,
        }),
    };

    let json = serde_json::to_value(&health).expect("serialize failed");

    // Root level fields
    assert_eq!(json.get("status").and_then(|v| v.as_str()), Some("healthy"));
    assert_eq!(json.get("version").and_then(|v| v.as_str()), Some("1.0.0"));

    // Nested models structure
    let models = json.get("models").unwrap();
    assert_eq!(
        models.get("total_models").and_then(|v| v.as_i64()),
        Some(10)
    );
    assert_eq!(models.get("loaded_count").and_then(|v| v.as_i64()), Some(8));
    assert_eq!(models.get("healthy").and_then(|v| v.as_bool()), Some(true));
}

#[tokio::test]
async fn test_adapter_list_response_structure() {
    // Frontend expects consistent structure for list responses
    let adapters = vec![
        AdapterResponse {
            id: "adapter-1".to_string(),
            name: "adapter-1-name".to_string(),
            version: "1.0.0".to_string(),
            status: "loaded".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-02T00:00:00Z".to_string(),
        },
        AdapterResponse {
            id: "adapter-2".to_string(),
            name: "adapter-2-name".to_string(),
            version: "2.0.0".to_string(),
            status: "unloaded".to_string(),
            created_at: "2024-02-01T00:00:00Z".to_string(),
            updated_at: "2024-02-02T00:00:00Z".to_string(),
        },
    ];

    // Serialize as array (typical for list endpoints)
    let json = serde_json::to_value(&adapters).expect("serialize failed");

    assert!(json.is_array(), "Should be array");
    assert_eq!(json.as_array().unwrap().len(), 2);

    // Check first element structure
    let first = &json[0];
    assert!(first.get("id").is_some());
    assert!(first.get("name").is_some());
    assert!(first.get("version").is_some());
}

// ============================================================================
// Cross-Version Compatibility
// ============================================================================

#[tokio::test]
async fn test_schema_version_field_consistency() {
    // All responses should have consistent schema_version handling
    let health = HealthResponse {
        schema_version: "1.0".to_string(),
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        models: None,
    };

    let error = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Test".to_string(),
        code: "TEST".to_string(),
        details: None,
    };

    let health_json = serde_json::to_value(&health).expect("serialize failed");
    let error_json = serde_json::to_value(&error).expect("serialize failed");

    // Both should have schema_version
    assert_eq!(
        health_json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0")
    );
    assert_eq!(
        error_json.get("schema_version").and_then(|v| v.as_str()),
        Some("1.0")
    );
}

#[tokio::test]
async fn test_timestamp_format_consistency() {
    // All timestamps should use consistent ISO 8601 format
    let adapter = AdapterResponse {
        id: "adapter-1".to_string(),
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T12:00:00Z".to_string(),
        updated_at: "2024-01-02T13:30:45Z".to_string(),
    };

    let json = serde_json::to_value(&adapter).expect("serialize failed");

    let created_at = json.get("created_at").and_then(|v| v.as_str()).unwrap();
    let updated_at = json.get("updated_at").and_then(|v| v.as_str()).unwrap();

    // Both should be ISO 8601
    assert!(is_iso8601(created_at));
    assert!(is_iso8601(updated_at));
}

fn is_iso8601(s: &str) -> bool {
    // Simplified check: contains T and Z/+/- for timezone
    s.contains('T') && (s.ends_with('Z') || s.contains('+') || s.contains('-'))
}
