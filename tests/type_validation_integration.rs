//! Integration test that demonstrates the complete type validation test suite
//!
//! This file shows how to use the type_validation module to validate:
//! 1. Round-trip serialization (Rust → JSON → Rust)
//! 2. OpenAPI schema compatibility
//! 3. Frontend type compatibility

mod type_validation;

use serde_json::json;
use type_validation::{is_valid_snake_case, validate_snake_case_fields};

#[tokio::test]
async fn test_type_validation_suite_integration() {
    // Example of using test utilities from the suite

    // Test 1: Validate snake_case compliance
    let valid_json = json!({
        "user_id": "123",
        "adapter_name": "code-assistant",
        "router_latency_us": 500
    });

    let violations = validate_snake_case_fields(&valid_json);
    assert!(
        violations.is_empty(),
        "Valid JSON should have no snake_case violations"
    );

    // Test 2: Validate individual field names
    assert!(is_valid_snake_case("input_tokens"));
    assert!(is_valid_snake_case("token_count"));
    assert!(is_valid_snake_case("_private_field"));
    assert!(is_valid_snake_case("value123"));

    // Invalid cases
    assert!(!is_valid_snake_case("inputTokens")); // camelCase not allowed
    assert!(!is_valid_snake_case("InputTokens")); // PascalCase not allowed
    assert!(!is_valid_snake_case("input-tokens")); // kebab-case not allowed
}

#[test]
fn test_suite_provides_comprehensive_validation() {
    // This test documents what the type_validation suite covers

    // The suite includes three modules:
    // 1. round_trip.rs
    //    - Tests Rust → JSON → Rust serialization round-trips
    //    - Validates field names use snake_case
    //    - Tests optional field handling
    //    - Verifies type precision (f64, i64, etc.)
    //    - Tests large payloads and complex nested structures

    // 2. openapi_compat.rs
    //    - Validates OpenAPI schema compliance
    //    - Ensures required fields are present
    //    - Verifies field types match schema
    //    - Validates field naming conventions
    //    - Tests timestamp formats (ISO 8601)
    //    - Validates array field consistency

    // 3. frontend_compat.rs
    //    - Validates TypeScript interface compatibility
    //    - Ensures all fields use snake_case
    //    - Verifies type compatibility (string, number, boolean, etc.)
    //    - Tests optional field omission behavior
    //    - Validates pagination structure
    //    - Tests health check response structure
    //    - Ensures consistency across versions
}

/// Example test pattern for adding new type validation
///
/// When adding a new type to the API, follow this pattern:
///
/// ```ignore
/// #[tokio::test]
/// async fn test_my_new_type_round_trip() {
///     let original = MyNewType {
///         field_a: "value".to_string(),
///         field_b: 42,
///     };
///
///     // Serialize to JSON
///     let json = serde_json::to_value(&original)
///         .expect("Failed to serialize");
///
///     // Deserialize back
///     let deserialized: MyNewType = serde_json::from_value(json.clone())
///         .expect("Failed to deserialize");
///
///     // Validate round-trip correctness
///     assert_eq!(original.field_a, deserialized.field_a);
///     assert_eq!(original.field_b, deserialized.field_b);
///
///     // Validate field naming
///     let violations = validate_snake_case_fields(&json);
///     assert!(violations.is_empty());
/// }
/// ```
#[test]
fn test_example_pattern_for_new_types() {
    // This test documents best practices
}

#[tokio::test]
async fn test_all_test_utilities_available() {
    // Verify all exported test utilities are accessible

    // Test utility 1: validate_snake_case_fields
    let json = json!({"user_id": "123"});
    let violations = validate_snake_case_fields(&json);
    assert!(violations.is_empty());

    // Test utility 2: is_valid_snake_case
    let valid = is_valid_snake_case("field_name");
    assert!(valid);

    // All utilities are exported from mod.rs and available for use
}
