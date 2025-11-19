//! Comprehensive schema validation test suite for API types
//!
//! This module validates:
//! - Serialization/deserialization round-trips (Rust ↔ JSON)
//! - OpenAPI schema compatibility
//! - Frontend type compatibility (snake_case field naming)
//! - Type consistency across crates
//!
//! ## Architecture
//!
//! The test suite is organized by validation concern:
//! - `round_trip.rs` - Serialization round-trip tests
//! - `openapi_compat.rs` - OpenAPI schema compatibility
//! - `frontend_compat.rs` - Frontend type compatibility

pub mod round_trip;
pub mod openapi_compat;
pub mod frontend_compat;

// Test utilities
mod test_utils {
    use serde_json::{json, Value};

    /// Helper to validate snake_case field naming
    pub fn validate_snake_case_fields(json_obj: &Value) -> Vec<String> {
        let mut violations = Vec::new();

        if let Some(obj) = json_obj.as_object() {
            for key in obj.keys() {
                if !is_valid_snake_case(key) {
                    violations.push(format!(
                        "Field '{}' is not in snake_case",
                        key
                    ));
                }
            }
        }

        violations
    }

    /// Check if string is valid snake_case
    pub fn is_valid_snake_case(s: &str) -> bool {
        // Allow empty strings
        if s.is_empty() {
            return false;
        }

        // Must start with lowercase or underscore
        let first_char = s.chars().next().unwrap();
        if !first_char.is_lowercase() && first_char != '_' {
            return false;
        }

        // Can only contain lowercase, digits, underscores
        s.chars().all(|c| c.is_lowercase() || c.is_numeric() || c == '_')
    }

    /// Helper to validate field type consistency
    pub fn validate_field_type(
        json_val: &Value,
        expected_type: &str,
    ) -> Result<(), String> {
        let actual_type = match json_val {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };

        if actual_type == expected_type {
            Ok(())
        } else {
            Err(format!(
                "Type mismatch: expected {}, got {}",
                expected_type, actual_type
            ))
        }
    }

    /// Recursively validate required fields are present
    pub fn validate_required_fields(
        json_obj: &Value,
        required_fields: &[&str],
    ) -> Vec<String> {
        let mut missing = Vec::new();

        if let Some(obj) = json_obj.as_object() {
            for field in required_fields {
                if !obj.contains_key(*field) {
                    missing.push(format!("Missing required field: {}", field));
                }
            }
        }

        missing
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_snake_case_validation() {
            assert!(is_valid_snake_case("user_id"));
            assert!(is_valid_snake_case("field_name"));
            assert!(is_valid_snake_case("_private"));
            assert!(is_valid_snake_case("test123"));

            assert!(!is_valid_snake_case("userId"));
            assert!(!is_valid_snake_case("FieldName"));
            assert!(!is_valid_snake_case(""));
            assert!(!is_valid_snake_case("field-name"));
        }

        #[test]
        fn test_validate_snake_case_fields() {
            let valid_json = json!({
                "user_id": "123",
                "field_name": "value",
                "another_field": 42
            });

            let violations = validate_snake_case_fields(&valid_json);
            assert!(violations.is_empty(), "Valid JSON should have no violations");

            let invalid_json = json!({
                "userId": "123",
                "field_Name": "value"
            });

            let violations = validate_snake_case_fields(&invalid_json);
            assert!(!violations.is_empty(), "Invalid JSON should have violations");
        }
    }
}

pub use test_utils::{
    is_valid_snake_case, validate_field_type, validate_required_fields,
    validate_snake_case_fields,
};
