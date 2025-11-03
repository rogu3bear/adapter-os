//! Content validation for secure file operations
//!
//! Provides validation functions for file content to prevent malformed
//! data from causing runtime crashes or security issues.

use adapteros_core::{AosError, Result};
use serde_json;

/// Validate that content is valid JSON before parsing
///
/// This prevents runtime crashes when loading configuration files
/// that may be corrupted or maliciously crafted.
///
/// # Arguments
/// * `content` - The content to validate as JSON
/// * `file_name` - Name of the file for error reporting
///
/// # Returns
/// * `Ok(())` if content is valid JSON
/// * `Err(AosError::Validation)` if content is invalid JSON
pub fn validate_json_content(content: &str, file_name: &str) -> Result<()> {
    // First, check that the content is not empty
    if content.trim().is_empty() {
        return Err(AosError::Validation(format!(
            "File '{}' contains only whitespace or is empty",
            file_name
        )));
    }

    // Attempt to parse as JSON to validate structure
    // We don't need the parsed value, just validation that it's valid JSON
    serde_json::from_str::<serde_json::Value>(content)
        .map_err(|e| AosError::Validation(format!(
            "File '{}' contains invalid JSON: {}",
            file_name, e
        )))?;

    Ok(())
}

/// Validate JSON content and parse it into a specific type
///
/// This combines validation and parsing for convenience.
///
/// # Arguments
/// * `content` - The content to validate and parse as JSON
/// * `file_name` - Name of the file for error reporting
///
/// # Returns
/// * `Ok(T)` if content is valid JSON and successfully parsed
/// * `Err(AosError::Validation)` if content is invalid JSON
pub fn validate_and_parse_json<T>(content: &str, file_name: &str) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    // Validate first
    validate_json_content(content, file_name)?;

    // Parse into the target type
    serde_json::from_str(content)
        .map_err(|e| AosError::Validation(format!(
            "Failed to parse '{}' into expected type: {}",
            file_name, e
        )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_json_validation() {
        let valid_json = r#"{"key": "value", "number": 42}"#;
        assert!(validate_json_content(valid_json, "test.json").is_ok());
    }

    #[test]
    fn test_invalid_json_validation() {
        let invalid_json = r#"{"key": "value", "number": 42"#; // Missing closing brace
        let result = validate_json_content(invalid_json, "test.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid JSON"));
    }

    #[test]
    fn test_empty_content_validation() {
        let empty_content = "";
        let result = validate_json_content(empty_content, "test.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_whitespace_only_validation() {
        let whitespace_content = "   \n\t   ";
        let result = validate_json_content(whitespace_content, "test.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validate_and_parse_success() {
        let json_content = r#"{"name": "test", "value": 123}"#;

        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct TestStruct {
            name: String,
            value: i32,
        }

        let result: TestStruct = validate_and_parse_json(json_content, "test.json").unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 123);
    }

    #[test]
    fn test_validate_and_parse_invalid_json() {
        let invalid_json = r#"{"name": "test", "value":}"#; // Invalid value

        #[derive(serde::Deserialize)]
        struct TestStruct {
            name: String,
            value: i32,
        }

        let result: Result<TestStruct> = validate_and_parse_json(invalid_json, "test.json");
        assert!(result.is_err());
    }
}
