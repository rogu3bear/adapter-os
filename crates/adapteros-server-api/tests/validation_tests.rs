//! Comprehensive tests for API request validation
//!
//! Tests cover:
//! - Invalid request payloads
//! - Missing required fields
//! - Invalid field values
//! - Boundary conditions
//! - Proper error responses for validation failures

use adapteros_core::validation as core_validation;
use adapteros_server_api::types::{
    BatchInferRequest, CreateBatchJobRequest, DirectoryUpsertRequest, ImportModelRequest,
};
use adapteros_server_api::validation::{
    validate_adapter_id, validate_description, validate_file_paths, validate_hash_b3,
    validate_name, validate_repo_id, ResponseSchema, ResponseSchemaValidator,
};
use axum::http::StatusCode;
use serde_json::json;

// ===== Field-level Validation Tests =====

#[test]
fn test_validate_adapter_id_empty() {
    let result = validate_adapter_id("");
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_adapter_id_too_long() {
    let long_id = "a".repeat(65);
    let result = validate_adapter_id(&long_id);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("64 characters"));
}

#[test]
fn test_validate_adapter_id_invalid_chars() {
    let test_cases = vec![
        "adapter@id",      // @ symbol
        "adapter id",      // space
        "adapter/id",      // forward slash
        "adapter.id",      // dot
        "adapter#id",      // hash
        "adapter$id",      // dollar sign
        "адаптер",         // non-ASCII
    ];

    for invalid_id in test_cases {
        let result = validate_adapter_id(invalid_id);
        assert!(
            result.is_err(),
            "Expected '{}' to fail validation",
            invalid_id
        );
        let (status, response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response.0.error.contains("alphanumeric"));
    }
}

#[test]
fn test_validate_adapter_id_valid() {
    let valid_ids = vec![
        "adapter1",
        "my-adapter",
        "my_adapter",
        "adapter-123",
        "ADAPTER_ID",
        "a",
        "adapter-with-many-dashes-and-underscores_123",
    ];

    for valid_id in valid_ids {
        let result = validate_adapter_id(valid_id);
        assert!(result.is_ok(), "Expected '{}' to be valid", valid_id);
    }
}

#[test]
fn test_validate_adapter_id_boundary() {
    // Exactly 64 characters (max length)
    let boundary_id = "a".repeat(64);
    assert!(validate_adapter_id(&boundary_id).is_ok());

    // 1 character (min length, non-empty)
    assert!(validate_adapter_id("a").is_ok());
}

#[test]
fn test_validate_name_empty() {
    let result = validate_name("");
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_name_too_long() {
    let long_name = "a".repeat(129);
    let result = validate_name(&long_name);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("128 characters"));
}

#[test]
fn test_validate_name_invalid_chars() {
    let test_cases = vec![
        "name@example",  // @ symbol
        "name/example",  // forward slash
        "name.example",  // dot
        "name#example",  // hash
        "name$example",  // dollar sign
        "имя",           // non-ASCII
    ];

    for invalid_name in test_cases {
        let result = validate_name(invalid_name);
        assert!(
            result.is_err(),
            "Expected '{}' to fail validation",
            invalid_name
        );
        let (status, response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response.0.error.contains("alphanumeric"));
    }
}

#[test]
fn test_validate_name_valid() {
    let valid_names = vec![
        "My Adapter",
        "adapter-name",
        "adapter_name",
        "Adapter 123",
        "ADAPTER NAME",
        "a",
        "Name with spaces and-dashes_underscores 123",
    ];

    for valid_name in valid_names {
        let result = validate_name(valid_name);
        assert!(result.is_ok(), "Expected '{}' to be valid", valid_name);
    }
}

#[test]
fn test_validate_name_boundary() {
    // Exactly 128 characters (max length)
    let boundary_name = "a".repeat(128);
    assert!(validate_name(&boundary_name).is_ok());

    // 1 character (min length, non-empty)
    assert!(validate_name("a").is_ok());
}

#[test]
fn test_validate_hash_b3_missing_prefix() {
    let result = validate_hash_b3(&"a".repeat(64));
    assert!(result.is_ok()); // Auto-prefixed in API validator
}

#[test]
fn test_validate_hash_b3_wrong_length() {
    let test_cases = vec![
        "b3:abc".to_string(),              // Too short
        format!("b3:{}", "a".repeat(63)), // 63 chars
        format!("b3:{}", "a".repeat(65)), // 65 chars
    ];

    for invalid_hash in &test_cases {
        let result = validate_hash_b3(invalid_hash);
        assert!(
            result.is_err(),
            "Expected '{}' to fail validation",
            invalid_hash
        );
        let (status, _response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[test]
fn test_validate_hash_b3_invalid_hex() {
    let invalid_hashes = vec![
        "b3:".to_string() + &"g".repeat(64), // 'g' is not hex
        "b3:".to_string() + &"z".repeat(64), // 'z' is not hex
        "b3:".to_string() + &"!".repeat(64), // special char
    ];

    for invalid_hash in &invalid_hashes {
        let result = validate_hash_b3(invalid_hash);
        assert!(
            result.is_err(),
            "Expected '{}' to fail validation",
            invalid_hash
        );
        let (status, _response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[test]
fn test_validate_hash_b3_valid() {
    let valid_hashes = vec![
        format!("b3:{}", "a".repeat(64)),
        format!("b3:{}", "0".repeat(64)),
        format!("b3:{}", "f".repeat(64)),
        "b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
    ];

    for valid_hash in &valid_hashes {
        let result = validate_hash_b3(valid_hash);
        assert!(result.is_ok(), "Expected '{}' to be valid", valid_hash);
    }
}

#[test]
fn test_validate_hash_b3_auto_prefix() {
    // Without prefix - should be auto-prefixed
    let hash_without_prefix = "0".repeat(64);
    let result = validate_hash_b3(&hash_without_prefix);
    assert!(result.is_ok());
}

#[test]
fn test_validate_repo_id_empty() {
    let result = validate_repo_id("");
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_repo_id_too_long() {
    let long_id = "a".repeat(257);
    let result = validate_repo_id(&long_id);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("256 characters"));
}

#[test]
fn test_validate_repo_id_invalid_chars() {
    let test_cases = vec![
        "repo@id",      // @ symbol
        "repo id",      // space
        "repo#id",      // hash
        "repo$id",      // dollar sign
        "репо",         // non-ASCII
    ];

    for invalid_id in test_cases {
        let result = validate_repo_id(invalid_id);
        assert!(
            result.is_err(),
            "Expected '{}' to fail validation",
            invalid_id
        );
        let (status, _response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[test]
fn test_validate_repo_id_valid() {
    let valid_ids = vec![
        "repo",
        "my-repo",
        "my_repo",
        "org/repo",
        "org/repo-name",
        "github.com/org/repo",
        "repo.name",
        "repo-123",
        "a",
    ];

    for valid_id in valid_ids {
        let result = validate_repo_id(valid_id);
        assert!(result.is_ok(), "Expected '{}' to be valid", valid_id);
    }
}

#[test]
fn test_validate_repo_id_boundary() {
    // Exactly 256 characters (max length)
    let boundary_id = "a".repeat(256);
    assert!(validate_repo_id(&boundary_id).is_ok());

    // 1 character (min length, non-empty)
    assert!(validate_repo_id("a").is_ok());
}

#[test]
fn test_validate_description_empty() {
    // Empty descriptions are allowed in API validator (unlike core)
    let result = validate_description("");
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_description_whitespace_only() {
    let result = validate_description("   ");
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_description_too_long() {
    let long_desc = "a".repeat(10001);
    let result = validate_description(&long_desc);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("10000 characters"));
}

#[test]
fn test_validate_description_valid() {
    let valid_descriptions = vec![
        "A simple description",
        "Description with special chars: @#$%^&*()",
        "Multi-line\ndescription\nwith\nnewlines",
        "Description with unicode: 你好世界 🚀",
        "a",
        "   padded description   ",
    ];

    for valid_desc in valid_descriptions {
        let result = validate_description(valid_desc);
        assert!(result.is_ok(), "Expected '{}' to be valid", valid_desc);
    }
}

#[test]
fn test_validate_description_boundary() {
    // Exactly 10000 characters (max length)
    let boundary_desc = "a".repeat(10000);
    assert!(validate_description(&boundary_desc).is_ok());

    // 1 character (min length, non-empty)
    assert!(validate_description("a").is_ok());
}

#[test]
fn test_validate_file_paths_empty_array() {
    let result = validate_file_paths(&[]);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("empty"));
}

#[test]
fn test_validate_file_paths_too_many() {
    let paths: Vec<String> = (0..101).map(|i| format!("file{}.txt", i)).collect();
    let result = validate_file_paths(&paths);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("100"));
}

#[test]
fn test_validate_file_paths_empty_string() {
    let paths = vec!["".to_string()];
    let result = validate_file_paths(&paths);
    assert!(result.is_err());
    let (status, _response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn test_validate_file_paths_whitespace_only() {
    let paths = vec!["   ".to_string()];
    let result = validate_file_paths(&paths);
    assert!(result.is_err());
    let (status, _response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn test_validate_file_paths_too_long() {
    let long_path = "a".repeat(513);
    let paths = vec![long_path];
    let result = validate_file_paths(&paths);
    assert!(result.is_err());
    let (status, response) = result.unwrap_err();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.0.error.contains("512 characters"));
}

#[test]
fn test_validate_file_paths_absolute_path() {
    let test_cases = vec![
        vec!["/absolute/path.txt".to_string()],
        vec!["relative/path.txt".to_string(), "/absolute/path.txt".to_string()],
    ];

    for paths in test_cases {
        let result = validate_file_paths(&paths);
        assert!(result.is_err());
        let (status, response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response.0.error.contains("absolute"));
    }
}

#[test]
fn test_validate_file_paths_path_traversal() {
    let test_cases = vec![
        vec!["../etc/passwd".to_string()],
        vec!["dir/../../../etc/passwd".to_string()],
        vec!["safe.txt".to_string(), "unsafe/../file.txt".to_string()],
    ];

    for paths in test_cases {
        let result = validate_file_paths(&paths);
        assert!(result.is_err(), "Expected {:?} to fail validation", paths);
        let (status, response) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response.0.error.contains(".."));
    }
}

#[test]
fn test_validate_file_paths_valid() {
    let test_cases = vec![
        vec!["file.txt".to_string()],
        vec!["dir/file.txt".to_string()],
        vec!["dir/subdir/file.txt".to_string()],
        vec!["file1.txt".to_string(), "file2.txt".to_string()],
        vec!["a/b/c.txt".to_string(), "d/e/f.txt".to_string()],
    ];

    for paths in test_cases {
        let result = validate_file_paths(&paths);
        assert!(result.is_ok(), "Expected {:?} to be valid", paths);
    }
}

#[test]
fn test_validate_file_paths_boundary() {
    // Exactly 100 paths (max count)
    let paths: Vec<String> = (0..100).map(|i| format!("file{}.txt", i)).collect();
    assert!(validate_file_paths(&paths).is_ok());

    // 1 path (min count, non-empty)
    let paths = vec!["file.txt".to_string()];
    assert!(validate_file_paths(&paths).is_ok());

    // Exactly 512 characters (max path length)
    let boundary_path = "a".repeat(512);
    let paths = vec![boundary_path];
    assert!(validate_file_paths(&paths).is_ok());
}

// ===== Core Validation Tests =====

#[test]
fn test_core_validate_adapter_id_empty() {
    let result = core_validation::validate_adapter_id("");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_core_validate_adapter_id_valid() {
    assert!(core_validation::validate_adapter_id("valid-adapter-id").is_ok());
}

#[test]
fn test_core_validate_name_empty() {
    let result = core_validation::validate_name("");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_core_validate_name_valid() {
    assert!(core_validation::validate_name("Valid Name 123").is_ok());
}

#[test]
fn test_core_validate_hash_b3_missing_prefix() {
    let result = core_validation::validate_hash_b3(&"a".repeat(64));
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("b3:"));
}

#[test]
fn test_core_validate_hash_b3_valid() {
    let valid_hash = "b3:".to_string() + &"0".repeat(64);
    assert!(core_validation::validate_hash_b3(&valid_hash).is_ok());
}

#[test]
fn test_core_validate_repo_id_empty() {
    let result = core_validation::validate_repo_id("");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_core_validate_repo_id_valid() {
    assert!(core_validation::validate_repo_id("org/repo").is_ok());
}

#[test]
fn test_core_validate_description_too_long() {
    let long_desc = "a".repeat(1025);
    let result = core_validation::validate_description(&long_desc);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("1024"));
}

#[test]
fn test_core_validate_description_valid() {
    assert!(core_validation::validate_description("A valid description").is_ok());
    // Core allows empty descriptions
    assert!(core_validation::validate_description("").is_ok());
}

#[test]
fn test_core_validate_file_paths_empty() {
    let result = core_validation::validate_file_paths(&[]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("empty"));
}

#[test]
fn test_core_validate_file_paths_valid() {
    assert!(core_validation::validate_file_paths(&["file.txt".to_string()]).is_ok());
}

// ===== Response Schema Validation Tests =====

#[tokio::test]
async fn test_response_schema_validator_new() {
    let validator = ResponseSchemaValidator::new(None);
    assert!(validator.has_schema("inference_response"));
    assert!(validator.has_schema("model_list_response"));
    assert!(validator.has_schema("error_response"));
}

#[tokio::test]
async fn test_response_schema_register_duplicate() {
    let mut validator = ResponseSchemaValidator::new(None);

    let schema = ResponseSchema {
        name: "test_schema".to_string(),
        schema: json!({"type": "object"}),
        required: true,
        version: "1.0.0".to_string(),
    };

    assert!(validator.register_schema(schema.clone()).is_ok());
    assert!(validator.register_schema(schema).is_err());
}

#[tokio::test]
async fn test_response_schema_validate_missing_required_fields() {
    let validator = ResponseSchemaValidator::new(None);

    // Missing token_count and latency_ms
    let response = json!({
        "text": "Hello world"
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(!result.valid);
    assert!(result.errors.len() >= 2);
    assert!(result.errors.iter().any(|e| e.contains("token_count")));
    assert!(result.errors.iter().any(|e| e.contains("latency_ms")));
}

#[tokio::test]
async fn test_response_schema_validate_wrong_type() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "text": "Hello world",
        "token_count": "not_a_number",  // Should be integer
        "latency_ms": 150
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e.contains("token_count")));
}

#[tokio::test]
async fn test_response_schema_validate_valid_inference() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "text": "Hello world",
        "token_count": 10,
        "latency_ms": 150,
        "trace": {
            "cpid": "test-123",
            "input_tokens": [1, 2, 3],
            "generated_tokens": [4, 5, 6],
            "router_decisions": [],
            "evidence": ["doc1", "doc2"]
        }
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(result.valid);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_response_schema_validate_unknown_schema() {
    let validator = ResponseSchemaValidator::new(None);
    let response = json!({"test": "value"});

    let result = validator
        .validate_response(&response, "nonexistent_schema")
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_response_schema_validate_model_list() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "models": [
            {
                "id": "model-1",
                "name": "Test Model",
                "status": "active",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        ]
    });

    let result = validator
        .validate_response(&response, "model_list_response")
        .await
        .unwrap();

    assert!(result.valid);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_response_schema_validate_model_list_missing_models() {
    let validator = ResponseSchemaValidator::new(None);
    let response = json!({});

    let result = validator
        .validate_response(&response, "model_list_response")
        .await
        .unwrap();

    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e.contains("models")));
}

#[tokio::test]
async fn test_response_schema_validate_error_response() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "schema_version": "1.0.0",
        "error": "Something went wrong",
        "code": "ERROR_CODE",
        "details": {}
    });

    let result = validator
        .validate_response(&response, "error_response")
        .await
        .unwrap();

    assert!(result.valid);
    assert!(result.errors.is_empty());
}

#[tokio::test]
async fn test_response_schema_validate_error_response_missing_error() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "schema_version": "1.0.0",
        "code": "ERROR_CODE"
    });

    let result = validator
        .validate_response(&response, "error_response")
        .await
        .unwrap();

    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e.contains("error")));
}

#[tokio::test]
async fn test_response_schema_custom_registration() {
    let mut validator = ResponseSchemaValidator::new(None);

    let custom_schema = ResponseSchema {
        name: "custom_response".to_string(),
        schema: json!({
            "type": "object",
            "required": ["id", "status"],
            "properties": {
                "id": {"type": "string"},
                "status": {"type": "string"},
                "count": {"type": "integer"}
            }
        }),
        required: true,
        version: "1.0.0".to_string(),
    };

    assert!(validator.register_schema(custom_schema).is_ok());
    assert!(validator.has_schema("custom_response"));
}

#[tokio::test]
async fn test_response_schema_validation_metrics() {
    let validator = ResponseSchemaValidator::new(None);

    let response = json!({
        "text": "test",
        "token_count": 5,
        "latency_ms": 100
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(result.response_size > 0);
    assert!(result.validation_time_us > 0);
    assert_eq!(result.schema_name, "inference_response");
}

// ===== Edge Cases and Boundary Tests =====

#[test]
fn test_validation_unicode_handling() {
    // Names should reject non-ASCII
    assert!(validate_name("Hello 世界").is_err());

    // Descriptions should allow unicode
    assert!(validate_description("Hello 世界").is_ok());
}

#[test]
fn test_validation_special_chars() {
    // Test various special characters in different validators
    assert!(validate_adapter_id("test!id").is_err());
    assert!(validate_adapter_id("test@id").is_err());
    assert!(validate_adapter_id("test#id").is_err());

    assert!(validate_name("test!name").is_err());
    assert!(validate_name("test@name").is_err());

    assert!(validate_repo_id("org/repo").is_ok());
    assert!(validate_repo_id("github.com/org/repo").is_ok());
}

#[test]
fn test_validation_normalization() {
    // Test whitespace handling
    assert!(validate_description("  trimmed  ").is_ok());

    // Empty after trim should fail
    assert!(validate_description("   ").is_err());
}

#[test]
fn test_validation_case_sensitivity() {
    // All validators should be case-sensitive
    assert!(validate_adapter_id("ABC").is_ok());
    assert!(validate_adapter_id("abc").is_ok());
    assert!(validate_name("ABC").is_ok());
    assert!(validate_name("abc").is_ok());
}

#[tokio::test]
async fn test_response_schema_type_coercion() {
    let validator = ResponseSchemaValidator::new(None);

    // Test that type validation is strict
    let response = json!({
        "text": "test",
        "token_count": 10.5,  // Float instead of integer
        "latency_ms": 100
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    // Should accept numbers even if not strictly integer
    assert!(result.valid);
}

#[tokio::test]
async fn test_response_schema_optional_fields() {
    let validator = ResponseSchemaValidator::new(None);

    // Optional trace field
    let response = json!({
        "text": "test",
        "token_count": 10,
        "latency_ms": 100
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(result.valid);
}

#[tokio::test]
async fn test_response_schema_extra_fields() {
    let validator = ResponseSchemaValidator::new(None);

    // Extra fields should be allowed
    let response = json!({
        "text": "test",
        "token_count": 10,
        "latency_ms": 100,
        "extra_field": "should be ignored"
    });

    let result = validator
        .validate_response(&response, "inference_response")
        .await
        .unwrap();

    assert!(result.valid);
}

// ===== Request Payload Validation Tests =====

#[test]
fn test_batch_infer_request_empty_requests() {
    let json = json!({
        "requests": []
    });

    let result: Result<BatchInferRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.requests.len(), 0);
}

#[test]
fn test_batch_infer_request_missing_required_fields() {
    // Missing 'requests' field
    let json = json!({});

    let result: Result<BatchInferRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_batch_infer_item_request_missing_id() {
    let json = json!({
        "requests": [
            {
                "prompt": "test"
            }
        ]
    });

    let result: Result<BatchInferRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_batch_infer_request_valid() {
    let json = json!({
        "requests": [
            {
                "id": "req-1",
                "prompt": "Hello world",
                "max_tokens": 100
            }
        ]
    });

    let result: Result<BatchInferRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.requests.len(), 1);
    assert_eq!(request.requests[0].id, "req-1");
}

#[test]
fn test_create_batch_job_request_invalid_timeout() {
    let json = json!({
        "requests": [
            {
                "id": "req-1",
                "prompt": "test"
            }
        ],
        "timeout_secs": -1
    });

    // Negative timeout should deserialize but could be validated at runtime
    let result: Result<CreateBatchJobRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.timeout_secs, Some(-1));
}

#[test]
fn test_create_batch_job_request_invalid_max_concurrent() {
    let json = json!({
        "requests": [
            {
                "id": "req-1",
                "prompt": "test"
            }
        ],
        "max_concurrent": 0
    });

    let result: Result<CreateBatchJobRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.max_concurrent, Some(0));
}

#[test]
fn test_create_batch_job_request_valid() {
    let json = json!({
        "requests": [
            {
                "id": "req-1",
                "prompt": "test"
            }
        ],
        "timeout_secs": 300,
        "max_concurrent": 10
    });

    let result: Result<CreateBatchJobRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.timeout_secs, Some(300));
    assert_eq!(request.max_concurrent, Some(10));
}

#[test]
fn test_directory_upsert_request_missing_tenant_id() {
    let json = json!({
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_directory_upsert_request_missing_root() {
    let json = json!({
        "tenant_id": "tenant-1",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_directory_upsert_request_missing_path() {
    let json = json!({
        "tenant_id": "tenant-1",
        "root": "/path/to/root"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_directory_upsert_request_default_activate() {
    let json = json!({
        "tenant_id": "tenant-1",
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert!(!request.activate); // Default should be false
}

#[test]
fn test_directory_upsert_request_valid() {
    let json = json!({
        "tenant_id": "tenant-1",
        "root": "/path/to/root",
        "path": "relative/path",
        "activate": true
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.tenant_id, "tenant-1");
    assert_eq!(request.root, "/path/to/root");
    assert_eq!(request.path, "relative/path");
    assert!(request.activate);
}

#[test]
fn test_import_model_request_missing_required_fields() {
    // Missing all required fields
    let json = json!({});
    let result: Result<ImportModelRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());

    // Missing some required fields
    let json = json!({
        "name": "test-model"
    });
    let result: Result<ImportModelRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_import_model_request_optional_fields() {
    let json = json!({
        "name": "test-model",
        "hash_b3": "b3:0000000000000000000000000000000000000000000000000000000000000000",
        "config_hash_b3": "b3:0000000000000000000000000000000000000000000000000000000000000000",
        "tokenizer_hash_b3": "b3:0000000000000000000000000000000000000000000000000000000000000000",
        "tokenizer_cfg_hash_b3": "b3:0000000000000000000000000000000000000000000000000000000000000000"
    });

    let result: Result<ImportModelRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert!(request.license_hash_b3.is_none());
    assert!(request.metadata_json.is_none());
}

#[test]
fn test_import_model_request_valid() {
    let json = json!({
        "name": "test-model",
        "hash_b3": "b3:0000000000000000000000000000000000000000000000000000000000000000",
        "config_hash_b3": "b3:1111111111111111111111111111111111111111111111111111111111111111",
        "tokenizer_hash_b3": "b3:2222222222222222222222222222222222222222222222222222222222222222",
        "tokenizer_cfg_hash_b3": "b3:3333333333333333333333333333333333333333333333333333333333333333",
        "license_hash_b3": "b3:4444444444444444444444444444444444444444444444444444444444444444",
        "metadata_json": "{\"key\": \"value\"}"
    });

    let result: Result<ImportModelRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.name, "test-model");
    assert!(request.license_hash_b3.is_some());
    assert!(request.metadata_json.is_some());
}

#[test]
fn test_request_wrong_type_for_field() {
    // tenant_id should be string, not number
    let json = json!({
        "tenant_id": 123,
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_request_extra_fields_ignored() {
    let json = json!({
        "tenant_id": "tenant-1",
        "root": "/path/to/root",
        "path": "relative/path",
        "extra_field": "should be ignored",
        "another_extra": 123
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
}

#[test]
fn test_request_null_values_for_required_fields() {
    let json = json!({
        "tenant_id": null,
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_request_array_when_object_expected() {
    let json = json!([
        {
            "tenant_id": "tenant-1",
            "root": "/path/to/root",
            "path": "relative/path"
        }
    ]);

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn test_request_string_when_number_expected() {
    let json = json!({
        "requests": [
            {
                "id": "req-1",
                "prompt": "test"
            }
        ],
        "timeout_secs": "not a number"
    });

    let result: Result<CreateBatchJobRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

// ===== Boundary Value Tests for Requests =====

#[test]
fn test_batch_request_large_payload() {
    let large_requests: Vec<_> = (0..1000)
        .map(|i| {
            json!({
                "id": format!("req-{}", i),
                "prompt": "test"
            })
        })
        .collect();

    let json = json!({
        "requests": large_requests
    });

    let result: Result<BatchInferRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
    let request = result.unwrap();
    assert_eq!(request.requests.len(), 1000);
}

#[test]
fn test_request_very_long_string_fields() {
    let long_string = "a".repeat(100000);

    let json = json!({
        "tenant_id": long_string,
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
}

#[test]
fn test_request_unicode_in_string_fields() {
    let json = json!({
        "tenant_id": "租户-1",
        "root": "/путь/到/根",
        "path": "relative/路径"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
}

#[test]
fn test_request_special_chars_in_string_fields() {
    let json = json!({
        "tenant_id": "tenant@#$%^&*()_+-=[]{}|;:',.<>?/~`",
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_ok());
}

#[test]
fn test_request_zero_and_negative_numbers() {
    let test_cases = vec![
        (0, Some(0)),
        (-1, Some(-1)),
        (-999, Some(-999)),
        (i32::MAX, Some(i32::MAX)),
        (i32::MIN, Some(i32::MIN)),
    ];

    for (value, expected) in test_cases {
        let json = json!({
            "requests": [
                {
                    "id": "req-1",
                    "prompt": "test"
                }
            ],
            "timeout_secs": value
        });

        let result: Result<CreateBatchJobRequest, _> = serde_json::from_value(json);
        assert!(result.is_ok(), "Failed for value: {}", value);
        let request = result.unwrap();
        assert_eq!(request.timeout_secs, expected);
    }
}

// ===== Error Response Validation =====

#[test]
fn test_validation_error_contains_field_name() {
    // When a required field is missing, error should indicate which field
    let json = json!({
        "root": "/path/to/root",
        "path": "relative/path"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("tenant_id") || err_str.contains("field"),
        "Error message should indicate missing field: {}",
        err_str
    );
}

#[test]
fn test_validation_error_type_mismatch() {
    let json = json!({
        "tenant_id": "tenant-1",
        "root": "/path/to/root",
        "path": "relative/path",
        "activate": "not_a_boolean"
    });

    let result: Result<DirectoryUpsertRequest, _> = serde_json::from_value(json);
    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("bool") || err_str.contains("type"),
        "Error message should indicate type mismatch: {}",
        err_str
    );
}

// ===== Integration Tests for Validation Flow =====

#[test]
fn test_validation_error_response_format() {
    use adapteros_api_types::ErrorResponse;

    let err = ErrorResponse::new("Validation failed")
        .with_code("VALIDATION_ERROR")
        .with_details(json!({
            "field": "tenant_id",
            "constraint": "required"
        }));

    assert_eq!(err.code, "VALIDATION_ERROR");
    assert_eq!(err.error, "Validation failed");
    assert!(err.details.is_some());
}

#[test]
fn test_validation_error_with_multiple_issues() {
    use adapteros_api_types::ErrorResponse;

    let err = ErrorResponse::new("Multiple validation errors")
        .with_code("VALIDATION_ERROR")
        .with_details(json!({
            "errors": [
                {"field": "tenant_id", "issue": "required"},
                {"field": "root", "issue": "invalid_path"},
                {"field": "path", "issue": "contains_traversal"}
            ]
        }));

    assert!(err.details.is_some());
    let details = err.details.unwrap();
    assert!(details.get("errors").is_some());
    assert_eq!(details.get("errors").unwrap().as_array().unwrap().len(), 3);
}
