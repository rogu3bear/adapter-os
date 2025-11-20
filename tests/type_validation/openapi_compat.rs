//! OpenAPI schema compatibility tests
//!
//! Validates that Rust types are compatible with generated OpenAPI specs,
//! including field naming, types, and required fields.

use adapteros_api_types::*;
use adapteros_server_api::types::*;
use serde_json::{json, Value};

// ============================================================================
// OpenAPI Schema Compliance
// ============================================================================

/// Represents an OpenAPI schema definition for comparison
#[derive(Debug, Clone)]
struct OpenApiSchemaField {
    name: String,
    field_type: String,
    required: bool,
    format: Option<String>,
}

#[tokio::test]
async fn test_infer_response_openapi_compatible() {
    // Expected OpenAPI schema for InferResponse
    let expected_fields = vec![
        OpenApiSchemaField {
            name: "text".to_string(),
            field_type: "string".to_string(),
            required: true,
            format: None,
        },
        OpenApiSchemaField {
            name: "token_count".to_string(),
            field_type: "integer".to_string(),
            required: true,
            format: Some("int64".to_string()),
        },
        OpenApiSchemaField {
            name: "latency_ms".to_string(),
            field_type: "integer".to_string(),
            required: true,
            format: Some("int64".to_string()),
        },
        OpenApiSchemaField {
            name: "trace".to_string(),
            field_type: "object".to_string(),
            required: false,
            format: None,
        },
    ];

    let response = InferResponse {
        text: "test output".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Validate field presence and types
    if let Some(obj) = json.as_object() {
        for expected_field in &expected_fields {
            if expected_field.required {
                assert!(
                    obj.contains_key(&expected_field.name),
                    "Required field '{}' missing",
                    expected_field.name
                );
            }

            // Validate field types when present
            if let Some(value) = obj.get(&expected_field.name) {
                validate_json_type(value, &expected_field.field_type);
            }
        }
    }
}

#[tokio::test]
async fn test_error_response_openapi_compatible() {
    let expected_schema = json!({
        "schema_version": { "type": "string", "required": true },
        "error": { "type": "string", "required": true },
        "code": { "type": "string", "required": true },
        "details": { "type": "object", "required": false }
    });

    let error = ErrorResponse {
        schema_version: "1.0".to_string(),
        error: "Validation failed".to_string(),
        code: "BAD_REQUEST".to_string(),
        details: Some(json!({"field": "email"})),
    };

    let json = serde_json::to_value(&error).expect("serialize failed");

    // Verify all required fields are present
    assert_has_required_fields(&json, &["schema_version", "error", "code"]);
}

#[tokio::test]
async fn test_batch_infer_request_openapi_compatible() {
    // Verify requests field is present and is array
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

    assert!(json.get("requests").is_some(), "Missing 'requests' field");
    assert!(
        json.get("requests").unwrap().is_array(),
        "'requests' should be array type"
    );
}

#[tokio::test]
async fn test_batch_infer_item_response_structure() {
    // Validate the structure of BatchInferItemResponse
    let response = BatchInferItemResponse {
        id: "req-1".to_string(),
        response: Some(InferResponse {
            text: "output".to_string(),
            token_count: 10,
            latency_ms: 100,
            trace: None,
        }),
        error: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // All three fields should be present (response/error may be null)
    assert!(json.get("id").is_some());
    assert!(json.get("response").is_some());
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn test_pagination_params_schema_compatibility() {
    let params = PaginationParams { page: 2, limit: 50 };

    let json = serde_json::to_value(&params).expect("serialize failed");

    assert_eq!(json.get("page").and_then(|v| v.as_u64()), Some(2));
    assert_eq!(json.get("limit").and_then(|v| v.as_u64()), Some(50));
}

#[tokio::test]
async fn test_health_response_schema_completeness() {
    let health = HealthResponse {
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

    let json = serde_json::to_value(&health).expect("serialize failed");

    // Verify required fields
    assert_has_required_fields(&json, &["schema_version", "status", "version"]);

    // Verify nested object structure
    if let Some(models) = json.get("models") {
        assert!(models.is_object(), "'models' should be object type");
        assert_has_required_fields(
            models,
            &[
                "total_models",
                "loaded_count",
                "healthy",
                "inconsistencies_count",
            ],
        );
    }
}

#[tokio::test]
async fn test_adapter_response_openapi_compatible() {
    let adapter = AdapterResponse {
        id: "adapter-1".to_string(),
        name: "test-adapter".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&adapter).expect("serialize failed");

    assert_has_required_fields(
        &json,
        &[
            "id",
            "name",
            "version",
            "status",
            "created_at",
            "updated_at",
        ],
    );
}

#[tokio::test]
async fn test_routing_decision_openapi_compatible() {
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

    // Required fields for routing decision
    assert_has_required_fields(
        &json,
        &[
            "id",
            "tenant_id",
            "timestamp",
            "step",
            "entropy",
            "tau",
            "entropy_floor",
            "adapters_used",
            "reason",
            "trace_id",
        ],
    );

    // Verify array types
    assert!(
        json.get("adapters_used").unwrap().is_array(),
        "'adapters_used' should be array"
    );
}

// ============================================================================
// Type Validation Helpers
// ============================================================================

fn validate_json_type(value: &Value, expected_type: &str) {
    match expected_type {
        "string" => {
            assert!(
                value.is_string(),
                "Expected string, got {}",
                value_type_name(value)
            );
        }
        "integer" => {
            assert!(
                value.is_i64() || value.is_u64(),
                "Expected integer, got {}",
                value_type_name(value)
            );
        }
        "number" => {
            assert!(
                value.is_f64() || value.is_i64() || value.is_u64(),
                "Expected number, got {}",
                value_type_name(value)
            );
        }
        "boolean" => {
            assert!(
                value.is_boolean(),
                "Expected boolean, got {}",
                value_type_name(value)
            );
        }
        "array" => {
            assert!(
                value.is_array(),
                "Expected array, got {}",
                value_type_name(value)
            );
        }
        "object" => {
            assert!(
                value.is_object(),
                "Expected object, got {}",
                value_type_name(value)
            );
        }
        _ => panic!("Unknown type: {}", expected_type),
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn assert_has_required_fields(obj: &Value, required_fields: &[&str]) {
    if let Some(o) = obj.as_object() {
        for field in required_fields {
            assert!(
                o.contains_key(*field),
                "Required field '{}' missing from OpenAPI schema",
                field
            );
        }
    } else {
        panic!("Value is not an object");
    }
}

// ============================================================================
// Field Naming Validation (OpenAPI uses snake_case)
// ============================================================================

#[tokio::test]
async fn test_openapi_field_naming_snake_case() {
    let infer_response = InferResponse {
        text: "test".to_string(),
        token_count: 10,
        latency_ms: 100,
        trace: None,
    };

    let json = serde_json::to_value(&infer_response).expect("serialize failed");

    // OpenAPI specs typically use snake_case for field names
    if let Some(obj) = json.as_object() {
        for key in obj.keys() {
            // Verify snake_case: lowercase, digits, underscores only
            let is_snake_case = key
                .chars()
                .all(|c| c.is_lowercase() || c.is_numeric() || c == '_');

            assert!(
                is_snake_case,
                "Field '{}' is not in snake_case (OpenAPI standard)",
                key
            );
        }
    }
}

#[tokio::test]
async fn test_nested_object_field_naming() {
    let trace = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![1, 2, 3],
        generated_tokens: vec![4, 5],
        router_decisions: vec![],
        evidence: vec![],
    };

    let json = serde_json::to_value(&trace).expect("serialize failed");

    validate_nested_snake_case(&json);
}

fn validate_nested_snake_case(value: &Value) {
    if let Some(obj) = value.as_object() {
        for (key, val) in obj {
            let is_snake_case = key
                .chars()
                .all(|c| c.is_lowercase() || c.is_numeric() || c == '_');

            assert!(is_snake_case, "Field '{}' is not in snake_case", key);

            // Recursively validate nested objects
            if val.is_object() {
                validate_nested_snake_case(val);
            }
        }
    }
}

// ============================================================================
// Compatibility with Common OpenAPI Patterns
// ============================================================================

#[tokio::test]
async fn test_timestamp_format_iso8601() {
    // OpenAPI uses ISO 8601 format for timestamps
    let response = AdapterResponse {
        id: "adapter-1".to_string(),
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        status: "loaded".to_string(),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-02T12:30:45Z".to_string(),
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Validate ISO 8601 format
    if let Some(created_at) = json.get("created_at").and_then(|v| v.as_str()) {
        assert!(
            is_iso8601_timestamp(created_at),
            "created_at should be ISO 8601 format"
        );
    }

    if let Some(updated_at) = json.get("updated_at").and_then(|v| v.as_str()) {
        assert!(
            is_iso8601_timestamp(updated_at),
            "updated_at should be ISO 8601 format"
        );
    }
}

fn is_iso8601_timestamp(s: &str) -> bool {
    // Simplified ISO 8601 validation
    // Format: YYYY-MM-DDTHH:MM:SSZ or YYYY-MM-DDTHH:MM:SS+offset
    s.len() >= 19 && s.contains('T') && (s.ends_with('Z') || s.contains('+') || s.contains('-'))
}

#[tokio::test]
async fn test_numeric_precision_in_openapi() {
    // OpenAPI specifies precision requirements for numeric types
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
        entropy: 2.71828, // Should preserve decimal precision
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

    // Verify numeric fields are numbers (not strings)
    assert!(
        json.get("entropy").unwrap().is_f64(),
        "entropy should be numeric"
    );
    assert!(json.get("tau").unwrap().is_f64(), "tau should be numeric");
}

// ============================================================================
// Array Field Compatibility
// ============================================================================

#[tokio::test]
async fn test_array_field_consistency() {
    let trace = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![1, 2, 3, 4, 5],
        generated_tokens: vec![6, 7, 8],
        router_decisions: vec![],
        evidence: vec!["source1".to_string(), "source2".to_string()],
    };

    let json = serde_json::to_value(&trace).expect("serialize failed");

    // Verify array fields are arrays
    assert!(json.get("input_tokens").unwrap().is_array());
    assert!(json.get("generated_tokens").unwrap().is_array());
    assert!(json.get("router_decisions").unwrap().is_array());
    assert!(json.get("evidence").unwrap().is_array());

    // Verify array element types
    let input_tokens = json.get("input_tokens").unwrap().as_array().unwrap();
    for elem in input_tokens {
        assert!(elem.is_number(), "input_tokens should contain numbers");
    }

    let evidence = json.get("evidence").unwrap().as_array().unwrap();
    for elem in evidence {
        assert!(elem.is_string(), "evidence should contain strings");
    }
}

#[tokio::test]
async fn test_empty_arrays_preserved() {
    let trace = InferenceTrace {
        cpid: "test".to_string(),
        input_tokens: vec![],
        generated_tokens: vec![],
        router_decisions: vec![],
        evidence: vec![],
    };

    let json = serde_json::to_value(&trace).expect("serialize failed");

    // Empty arrays should be present, not omitted
    assert!(json.get("input_tokens").is_some());
    assert_eq!(
        json.get("input_tokens").unwrap().as_array().unwrap().len(),
        0
    );
}
