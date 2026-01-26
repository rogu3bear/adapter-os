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
    serde_json::from_str::<serde_json::Value>(content).map_err(|e| {
        AosError::Validation(format!("File '{}' contains invalid JSON: {}", file_name, e))
    })?;

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
    serde_json::from_str(content).map_err(|e| {
        AosError::Validation(format!(
            "Failed to parse '{}' into expected type: {}",
            file_name, e
        ))
    })
}

/// Validate model configuration JSON with semantic checks
///
/// Performs both syntax validation and semantic validation for model config.json files.
/// Checks for required fields and reasonable value ranges.
///
/// # Arguments
/// * `content` - The JSON content to validate
///
/// # Returns
/// * `Ok(())` if content is valid model config JSON
/// * `Err(AosError::Validation)` if content is invalid
pub fn validate_model_config_json(content: &str) -> Result<()> {
    // First do basic JSON validation
    validate_json_content(content, "config.json")?;

    // Parse as generic JSON to perform semantic validation
    let config: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| AosError::Validation(format!("Failed to parse config.json: {}", e)))?;

    // Validate required fields
    let required_fields = vec![
        "vocab_size",
        "hidden_size",
        "num_layers",
        "num_attention_heads",
        "intermediate_size",
    ];

    for field in required_fields {
        if config.get(field).is_none() {
            return Err(AosError::Validation(format!(
                "config.json missing required field: {}",
                field
            )));
        }
    }

    // Validate field types and ranges
    if let Some(vocab_size) = config.get("vocab_size") {
        if let Some(size) = vocab_size.as_u64() {
            if size == 0 || size > 200_000 {
                return Err(AosError::Validation(format!(
                    "config.json vocab_size {} is out of reasonable range (1-200,000)",
                    size
                )));
            }
        } else {
            return Err(AosError::Validation(
                "config.json vocab_size must be a positive integer".to_string(),
            ));
        }
    }

    if let Some(hidden_size) = config.get("hidden_size") {
        if let Some(size) = hidden_size.as_u64() {
            if !(128..=16384).contains(&size) {
                return Err(AosError::Validation(format!(
                    "config.json hidden_size {} is out of reasonable range (128-16,384)",
                    size
                )));
            }
        } else {
            return Err(AosError::Validation(
                "config.json hidden_size must be a positive integer".to_string(),
            ));
        }
    }

    if let Some(num_layers) = config.get("num_layers") {
        if let Some(layers) = num_layers.as_u64() {
            if layers == 0 || layers > 256 {
                return Err(AosError::Validation(format!(
                    "config.json num_layers {} is out of reasonable range (1-256)",
                    layers
                )));
            }
        } else {
            return Err(AosError::Validation(
                "config.json num_layers must be a positive integer".to_string(),
            ));
        }
    }

    if let Some(num_heads) = config.get("num_attention_heads") {
        if let Some(heads) = num_heads.as_u64() {
            if heads == 0 || heads > 128 {
                return Err(AosError::Validation(format!(
                    "config.json num_attention_heads {} is out of reasonable range (1-128)",
                    heads
                )));
            }
        } else {
            return Err(AosError::Validation(
                "config.json num_attention_heads must be a positive integer".to_string(),
            ));
        }
    }

    Ok(())
}

/// Validate tokenizer configuration JSON with semantic checks
///
/// Performs validation for tokenizer.json files to ensure they contain
/// valid tokenizer configuration.
///
/// # Arguments
/// * `content` - The JSON content to validate
///
/// # Returns
/// * `Ok(())` if content is valid tokenizer config JSON
/// * `Err(AosError::Validation)` if content is invalid
pub fn validate_tokenizer_config_json(content: &str) -> Result<()> {
    // First do basic JSON validation
    validate_json_content(content, "tokenizer.json")?;

    // Parse as generic JSON
    let config: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| AosError::Validation(format!("Failed to parse tokenizer.json: {}", e)))?;

    // Tokenizers should have some basic structure
    // At minimum, check for common tokenizer fields
    let has_common_fields = config.get("model").is_some()
        || config.get("vocab").is_some()
        || config.get("merges").is_some()
        || config.get("added_tokens").is_some();

    if !has_common_fields {
        return Err(AosError::Validation(
            "tokenizer.json does not appear to contain valid tokenizer configuration".to_string(),
        ));
    }

    // If it has a model field, validate it's a known type
    if let Some(model) = config.get("model") {
        if let Some(model_type) = model.get("type") {
            if let Some(type_str) = model_type.as_str() {
                let valid_types = [
                    "BPE",
                    "WordPiece",
                    "Unigram",
                    "BBPE",
                    "GPT2",
                    "Llama",
                    "Qwen2",
                    "T5",
                ];
                if !valid_types.contains(&type_str) {
                    return Err(AosError::Validation(format!(
                        "tokenizer.json contains unknown model type: {}",
                        type_str
                    )));
                }
            }
        }
    }

    Ok(())
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
            _name: String,
            _value: i32,
        }

        let result: Result<TestStruct> = validate_and_parse_json(invalid_json, "test.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_model_config_success() {
        let valid_config = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096,
            "num_layers": 32,
            "num_attention_heads": 32,
            "intermediate_size": 14336,
            "max_position_embeddings": 32768
        }"#;

        assert!(validate_model_config_json(valid_config).is_ok());
    }

    #[test]
    fn test_validate_model_config_missing_field() {
        let invalid_config = r#"{
            "vocab_size": 32000,
            "hidden_size": 4096
        }"#; // Missing required fields

        let result = validate_model_config_json(invalid_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing required field"));
    }

    #[test]
    fn test_validate_model_config_invalid_range() {
        let invalid_config = r#"{
            "vocab_size": 500000,
            "hidden_size": 4096,
            "num_layers": 32,
            "num_attention_heads": 32,
            "intermediate_size": 14336
        }"#; // vocab_size too large

        let result = validate_model_config_json(invalid_config);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("out of reasonable range"));
    }

    #[test]
    fn test_validate_tokenizer_config_success() {
        let valid_tokenizer = r#"{
            "model": {
                "type": "BPE",
                "vocab": {},
                "merges": []
            },
            "added_tokens": []
        }"#;

        assert!(validate_tokenizer_config_json(valid_tokenizer).is_ok());
    }

    #[test]
    fn test_validate_tokenizer_config_invalid_type() {
        let invalid_tokenizer = r#"{
            "model": {
                "type": "InvalidType"
            }
        }"#;

        let result = validate_tokenizer_config_json(invalid_tokenizer);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown model type"));
    }

    #[test]
    fn test_validate_tokenizer_config_missing_structure() {
        let invalid_tokenizer = r#"{
            "some_field": "value"
        }"#; // No tokenizer structure

        let result = validate_tokenizer_config_json(invalid_tokenizer);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not appear to contain valid tokenizer"));
    }
}
