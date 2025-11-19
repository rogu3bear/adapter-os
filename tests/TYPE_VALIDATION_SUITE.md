# Type Validation Test Suite

## Overview

The type validation test suite provides comprehensive testing for API type compatibility across the full stack: Rust types → JSON → TypeScript interfaces.

This ensures:
- Serialization round-trips preserve all data (Rust → JSON → Rust)
- OpenAPI schema compliance
- TypeScript frontend compatibility
- Consistent field naming (snake_case)
- Type precision preservation
- Optional field handling

## Architecture

The test suite is organized into three specialized modules:

### 1. Round-Trip Serialization Tests (`round_trip.rs`)

Tests that Rust types can be serialized to JSON and deserialized back without data loss.

**Coverage:**
- Basic types (strings, numbers, booleans)
- Complex nested structures
- Optional fields
- Arrays and collections
- Numeric precision (f64, i64)
- Large payloads
- Field naming consistency

**Example:**
```rust
#[tokio::test]
async fn test_infer_response_round_trip() {
    let original = InferResponse {
        text: "The quick brown fox".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: InferResponse = serde_json::from_value(json)
        .expect("deserialize failed");

    assert_eq!(original.text, deserialized.text);
    assert_eq!(original.token_count, deserialized.token_count);
}
```

### 2. OpenAPI Schema Compatibility Tests (`openapi_compat.rs`)

Validates that Rust types are compatible with generated OpenAPI specifications.

**Coverage:**
- Required field presence
- Field type correctness
- Field naming conventions (snake_case)
- Nested object structure
- Array field consistency
- Timestamp format (ISO 8601)
- Numeric precision in schemas
- Empty array preservation

**Example:**
```rust
#[tokio::test]
async fn test_infer_response_openapi_compatible() {
    let response = InferResponse {
        text: "test output".to_string(),
        token_count: 42,
        latency_ms: 150,
        trace: None,
    };

    let json = serde_json::to_value(&response).expect("serialize failed");

    // Verify required fields are present
    assert!(json.get("text").is_some());
    assert!(json.get("token_count").is_some());
    assert!(json.get("latency_ms").is_some());
}
```

### 3. Frontend Type Compatibility Tests (`frontend_compat.rs`)

Ensures API responses match TypeScript interface definitions in the frontend.

**Coverage:**
- Field naming: snake_case consistency
- Type compatibility (string, number, boolean)
- Optional field handling (null vs. omitted)
- Array field types
- Integer field serialization
- Float field serialization
- Timestamp format consistency
- Pagination structure
- Health check response structure

**Example:**
```rust
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
    assert!(violations.is_empty());
}
```

## Test Utilities

The suite provides helper functions for custom tests:

### `validate_snake_case_fields(json_obj: &Value) -> Vec<String>`

Validates that all fields in a JSON object use snake_case naming.

```rust
let json = json!({"user_id": "123", "field_name": "value"});
let violations = validate_snake_case_fields(&json);
assert!(violations.is_empty());
```

### `is_valid_snake_case(s: &str) -> bool`

Checks if a string is valid snake_case.

```rust
assert!(is_valid_snake_case("user_id"));
assert!(is_valid_snake_case("_private"));
assert!(!is_valid_snake_case("userId"));  // camelCase not allowed
```

### `validate_field_type(json_val: &Value, expected_type: &str) -> Result<(), String>`

Validates JSON value matches expected type.

```rust
let value = json!("test");
validate_field_type(&value, "string").unwrap();
```

### `validate_required_fields(json_obj: &Value, required_fields: &[&str]) -> Vec<String>`

Ensures required fields are present in JSON object.

```rust
let obj = json!({"id": "123"});
let missing = validate_required_fields(&obj, &["id", "name"]);
assert_eq!(missing.len(), 1);  // "name" is missing
```

## Running the Tests

### Run all type validation tests:
```bash
cargo test --test type_validation_integration
cargo test round_trip::
cargo test openapi_compat::
cargo test frontend_compat::
```

### Run specific test:
```bash
cargo test test_infer_response_round_trip
cargo test test_batch_infer_request_round_trip
cargo test test_field_names_use_snake_case
```

### Run with output:
```bash
cargo test --test type_validation_integration -- --nocapture
```

## Adding New Type Validation Tests

When adding a new API type, follow this pattern:

### Step 1: Add to round_trip.rs
```rust
#[tokio::test]
async fn test_my_new_type_round_trip() {
    let original = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
        field_c: vec![1.0, 2.0, 3.0],
    };

    let json = serde_json::to_value(&original)
        .expect("Failed to serialize");

    let deserialized: MyNewType = serde_json::from_value(json.clone())
        .expect("Failed to deserialize");

    // Validate round-trip correctness
    assert_eq!(original.field_a, deserialized.field_a);
    assert_eq!(original.field_b, deserialized.field_b);
    assert_eq!(original.field_c, deserialized.field_c);

    // Validate field naming
    let violations = validate_snake_case_fields(&json);
    assert!(violations.is_empty(), "Violations: {:?}", violations);
}
```

### Step 2: Add to openapi_compat.rs
```rust
#[tokio::test]
async fn test_my_new_type_openapi_compatible() {
    let value = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
        field_c: vec![1.0, 2.0, 3.0],
    };

    let json = serde_json::to_value(&value)
        .expect("serialize failed");

    // Verify required fields are present
    assert_has_required_fields(&json, &["field_a", "field_b", "field_c"]);

    // Verify types
    assert!(json.get("field_a").unwrap().is_string());
    assert!(json.get("field_b").unwrap().is_i64());
    assert!(json.get("field_c").unwrap().is_array());
}
```

### Step 3: Add to frontend_compat.rs
```rust
#[tokio::test]
async fn test_my_new_type_frontend_compatible() {
    let value = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
        field_c: vec![1.0, 2.0, 3.0],
    };

    let json = serde_json::to_value(&value)
        .expect("serialize failed");

    // Validate field naming
    let violations = validate_all_fields_snake_case(&json, "MyNewType");
    assert!(violations.is_empty());

    // Validate type compatibility
    assert!(json.get("field_a").unwrap().is_string());
    assert!(json.get("field_b").unwrap().is_number());
    assert!(json.get("field_c").unwrap().is_array());
}
```

## Common Test Patterns

### Testing Optional Fields

```rust
#[tokio::test]
async fn test_optional_field_omitted() {
    let with_none = MyType {
        required_field: "value".to_string(),
        optional_field: None,
    };

    let json = serde_json::to_value(&with_none).expect("serialize failed");
    assert!(!json.as_object().unwrap().contains_key("optional_field"));
}

#[tokio::test]
async fn test_optional_field_present() {
    let with_some = MyType {
        required_field: "value".to_string(),
        optional_field: Some("optional_value".to_string()),
    };

    let json = serde_json::to_value(&with_some).expect("serialize failed");
    assert!(json.as_object().unwrap().contains_key("optional_field"));
}
```

### Testing Numeric Precision

```rust
#[tokio::test]
async fn test_float_precision() {
    let original = MyType {
        precision_value: 3.14159265359,
    };

    let json = serde_json::to_value(&original).expect("serialize failed");
    let deserialized: MyType = serde_json::from_value(json)
        .expect("deserialize failed");

    // Check precision is maintained within acceptable bounds
    assert!((original.precision_value - deserialized.precision_value).abs() < 1e-10);
}
```

### Testing Arrays

```rust
#[tokio::test]
async fn test_array_fields() {
    let original = MyType {
        items: vec!["a".to_string(), "b".to_string(), "c".to_string()],
    };

    let json = serde_json::to_value(&original).expect("serialize failed");

    // Verify array type
    assert!(json.get("items").unwrap().is_array());

    // Verify array length
    assert_eq!(json.get("items").unwrap().as_array().unwrap().len(), 3);

    // Verify element types
    for elem in json.get("items").unwrap().as_array().unwrap() {
        assert!(elem.is_string());
    }
}
```

## Test Coverage

The suite currently covers:

### Inference Types
- `InferResponse` - Round-trip, OpenAPI, Frontend
- `InferRequest` - Round-trip
- `InferenceTrace` - Round-trip, OpenAPI, Frontend
- `RouterDecision` - Round-trip, OpenAPI, Frontend
- `BatchInferRequest` - Round-trip, OpenAPI
- `BatchInferResponse` - Round-trip, OpenAPI

### Adapter Types
- `AdapterResponse` - Round-trip, OpenAPI, Frontend
- `AdapterManifest` - Round-trip

### Error & Status Types
- `ErrorResponse` - Round-trip, OpenAPI, Frontend
- `HealthResponse` - Round-trip, OpenAPI, Frontend
- `ModelRuntimeHealth` - Round-trip, OpenAPI

### Other Types
- `PaginationParams` - OpenAPI, Frontend
- `PaginatedResponse<T>` - Round-trip, Frontend
- `RoutingDecision` - Round-trip, OpenAPI, Frontend

## Field Naming Standards

All API types must use **snake_case** for JSON field names:

### Valid
- `user_id`
- `token_count`
- `latency_ms`
- `router_latency_us`
- `input_tokens`
- `_private_field`
- `field123`

### Invalid
- `userId` (camelCase)
- `UserID` (PascalCase)
- `user-id` (kebab-case)
- `user.id` (dot notation)

Use `#[serde(rename = "snake_case")]` to ensure correct serialization:

```rust
#[derive(Serialize, Deserialize)]
pub struct MyType {
    #[serde(rename = "user_id")]
    pub user_id: String,

    #[serde(rename = "token_count")]
    pub token_count: i64,
}
```

## Troubleshooting

### Test Fails: "Field 'X' is not in snake_case"
**Cause:** A Rust field was serialized with incorrect naming.

**Solution:** Add serde rename attribute or update field name:
```rust
#[serde(rename = "correct_name")]
pub field_name: String,
```

### Test Fails: "Type mismatch"
**Cause:** A field was serialized to wrong JSON type (e.g., string instead of number).

**Solution:** Ensure correct Rust type or use custom serializer:
```rust
#[serde(serialize_with = "serialize_as_number")]
pub field: String,
```

### Test Fails: "Required field missing"
**Cause:** A field that should always be present was omitted.

**Solution:** Remove `#[serde(skip_serializing_if)]` or ensure field is always set.

## Integration with CI/CD

Add to your CI/CD pipeline:

```bash
# Run all type validation tests
cargo test --test type_validation_integration

# Run with strict linting
cargo clippy --tests -- -D warnings

# Run with code coverage
cargo tarpaulin --test type_validation_integration
```

## Related Documentation

- **OpenAPI Specification:** See generated docs with `cargo doc --open`
- **Frontend Types:** `/ui/src/api/types.ts`
- **Serde Documentation:** https://serde.rs/
- **JSON Schema Specification:** https://json-schema.org/

## Future Enhancements

- [ ] Property-based testing with `proptest`
- [ ] Fuzzing with `cargo-fuzz`
- [ ] Type-level validation using `typenum`
- [ ] Cross-version compatibility tests
- [ ] Performance benchmarks for serialization
- [ ] OpenAPI spec generation validation
- [ ] TypeScript definition generation validation
