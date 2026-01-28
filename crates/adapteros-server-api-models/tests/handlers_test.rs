//! Unit tests for adapteros-server-api-models handlers
//!
//! Tests verify type serialization for model API types.
//! Note: Some types only implement Serialize (not Deserialize) as they are
//! response-only types. Tests focus on serialization and structural correctness.

use adapteros_server_api_models::handlers::{
    AneMemoryStatus, ModelDownloadProgress, ModelStatusResponse, ModelValidationResponse,
    SeedModelRequest, SeedModelResponse, ValidationIssue,
};
use adapteros_api_types::ModelLoadStatus;
use serde_json;

/// Test SeedModelRequest serialization and deserialization
#[test]
fn test_seed_model_request_serde() {
    let request = SeedModelRequest {
        model_name: "Llama-3-8B".to_string(),
        model_path: "/var/models/llama-3-8b".to_string(),
        format: "mlx".to_string(),
        backend: "mlx".to_string(),
        capabilities: Some(vec!["chat".to_string(), "completion".to_string()]),
        metadata: Some(serde_json::json!({"quantization": "q4"})),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&request).expect("Failed to serialize SeedModelRequest");
    assert!(json.contains("Llama-3-8B"));
    assert!(json.contains("/var/models/llama-3-8b"));
    assert!(json.contains("mlx"));
    assert!(json.contains("chat"));

    // Deserialize back
    let deserialized: SeedModelRequest =
        serde_json::from_str(&json).expect("Failed to deserialize SeedModelRequest");
    assert_eq!(deserialized.model_name, request.model_name);
    assert_eq!(deserialized.model_path, request.model_path);
    assert_eq!(deserialized.format, request.format);
    assert_eq!(deserialized.backend, request.backend);
    assert_eq!(deserialized.capabilities, request.capabilities);
}

/// Test SeedModelRequest with minimal fields
#[test]
fn test_seed_model_request_minimal_serde() {
    let request = SeedModelRequest {
        model_name: "minimal-model".to_string(),
        model_path: "/models/minimal".to_string(),
        format: "safetensors".to_string(),
        backend: "metal".to_string(),
        capabilities: None,
        metadata: None,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&request).expect("Failed to serialize SeedModelRequest");
    assert!(json.contains("minimal-model"));

    // Deserialize back
    let deserialized: SeedModelRequest =
        serde_json::from_str(&json).expect("Failed to deserialize SeedModelRequest");
    assert_eq!(deserialized.model_name, request.model_name);
    assert!(deserialized.capabilities.is_none());
    assert!(deserialized.metadata.is_none());
}

/// Test SeedModelResponse serialization and deserialization
#[test]
fn test_seed_model_response_serde() {
    let response = SeedModelResponse {
        import_id: "import-12345".to_string(),
        status: "available".to_string(),
        message: "Model import completed".to_string(),
        progress: Some(100),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&response).expect("Failed to serialize SeedModelResponse");
    assert!(json.contains("import-12345"));
    assert!(json.contains("available"));
    assert!(json.contains("100"));

    // Deserialize back
    let deserialized: SeedModelResponse =
        serde_json::from_str(&json).expect("Failed to deserialize SeedModelResponse");
    assert_eq!(deserialized.import_id, response.import_id);
    assert_eq!(deserialized.status, response.status);
    assert_eq!(deserialized.message, response.message);
    assert_eq!(deserialized.progress, Some(100));
}

/// Test ModelStatusResponse serialization and deserialization
#[test]
fn test_model_status_response_serde() {
    let response = ModelStatusResponse {
        model_id: "qwen-7b".to_string(),
        model_name: "Qwen2.5-7B-Instruct".to_string(),
        model_path: Some("/var/models/qwen2.5-7b".to_string()),
        status: ModelLoadStatus::Ready,
        loaded_at: Some("2025-01-28T12:00:00Z".to_string()),
        error_message: None,
        memory_usage_mb: Some(4096),
        is_loaded: true,
        ane_memory: None,
        uma_pressure_level: Some("low".to_string()),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&response).expect("Failed to serialize ModelStatusResponse");
    assert!(json.contains("qwen-7b"));
    assert!(json.contains("Qwen2.5-7B-Instruct"));
    assert!(json.contains("4096"));

    // Deserialize back
    let deserialized: ModelStatusResponse =
        serde_json::from_str(&json).expect("Failed to deserialize ModelStatusResponse");
    assert_eq!(deserialized.model_id, response.model_id);
    assert_eq!(deserialized.model_name, response.model_name);
    assert_eq!(deserialized.is_loaded, true);
    assert_eq!(deserialized.memory_usage_mb, Some(4096));
}

/// Test ModelStatusResponse with all optional fields
#[test]
fn test_model_status_response_with_ane_memory() {
    let ane_memory = AneMemoryStatus {
        allocated_mb: 2048,
        used_mb: 1024,
        available_mb: 1024,
        usage_pct: 50.0,
    };

    let response = ModelStatusResponse {
        model_id: "test-model".to_string(),
        model_name: "Test Model".to_string(),
        model_path: None,
        status: ModelLoadStatus::Loading,
        loaded_at: None,
        error_message: None,
        memory_usage_mb: None,
        is_loaded: false,
        ane_memory: Some(ane_memory),
        uma_pressure_level: Some("medium".to_string()),
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("allocated_mb"));
    assert!(json.contains("2048"));
    assert!(json.contains("usage_pct"));
}

/// Test AneMemoryStatus serialization and deserialization
#[test]
fn test_ane_memory_status_serde() {
    let status = AneMemoryStatus {
        allocated_mb: 4096,
        used_mb: 2048,
        available_mb: 2048,
        usage_pct: 50.0,
    };

    let json = serde_json::to_string(&status).expect("Failed to serialize AneMemoryStatus");
    assert!(json.contains("4096"));
    assert!(json.contains("50"));

    let deserialized: AneMemoryStatus =
        serde_json::from_str(&json).expect("Failed to deserialize AneMemoryStatus");
    assert_eq!(deserialized.allocated_mb, 4096);
    assert_eq!(deserialized.used_mb, 2048);
    assert_eq!(deserialized.available_mb, 2048);
    assert!((deserialized.usage_pct - 50.0).abs() < f32::EPSILON);
}

/// Test ModelValidationResponse serialization
/// Note: ModelValidationResponse only implements Serialize, not Deserialize
#[test]
fn test_model_validation_response_serialize() {
    let response = ModelValidationResponse {
        model_id: "test-model".to_string(),
        status: "ready".to_string(),
        valid: true,
        can_load: true,
        reason: None,
        issues: vec![],
        errors: vec![],
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("test-model"));
    assert!(json.contains("ready"));
    assert!(json.contains("true"));

    // Verify JSON structure
    let value: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
    assert_eq!(value["model_id"], "test-model");
    assert_eq!(value["valid"], true);
    assert_eq!(value["can_load"], true);
}

/// Test ModelValidationResponse with issues
/// Note: ModelValidationResponse only implements Serialize, not Deserialize
#[test]
fn test_model_validation_response_with_issues() {
    let issues = vec![
        ValidationIssue {
            issue_type: "hash_mismatch".to_string(),
            message: "Config hash does not match".to_string(),
        },
        ValidationIssue {
            issue_type: "missing_file".to_string(),
            message: "Tokenizer config not found".to_string(),
        },
    ];

    let response = ModelValidationResponse {
        model_id: "invalid-model".to_string(),
        status: "invalid".to_string(),
        valid: false,
        can_load: false,
        reason: Some("Validation failed".to_string()),
        issues,
        errors: vec!["Config hash does not match".to_string()],
    };

    let json = serde_json::to_string(&response).expect("Failed to serialize");
    assert!(json.contains("invalid-model"));
    assert!(json.contains("hash_mismatch"));
    assert!(json.contains("missing_file"));

    // Verify JSON structure
    let value: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
    assert_eq!(value["valid"], false);
    assert_eq!(value["issues"].as_array().unwrap().len(), 2);
    assert_eq!(value["errors"].as_array().unwrap().len(), 1);
}

/// Test ValidationIssue serialization
/// Note: ValidationIssue only implements Serialize, not Deserialize
#[test]
fn test_validation_issue_serialize() {
    let issue = ValidationIssue {
        issue_type: "validation_error".to_string(),
        message: "Model weights hash is missing".to_string(),
    };

    let json = serde_json::to_string(&issue).expect("Failed to serialize ValidationIssue");
    // Note: issue_type is renamed to "type" in JSON
    assert!(json.contains("type"));
    assert!(json.contains("validation_error"));
    assert!(json.contains("Model weights hash is missing"));

    // Verify JSON structure
    let value: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
    assert_eq!(value["type"], "validation_error");
    assert_eq!(value["message"], "Model weights hash is missing");
}

/// Test ModelDownloadProgress serialization
#[test]
fn test_model_download_progress_serde() {
    let progress = ModelDownloadProgress {
        model_id: "qwen-7b".to_string(),
        operation_id: "op-12345".to_string(),
        operation: "import".to_string(),
        status: "in_progress".to_string(),
        started_at: "2025-01-28T12:00:00Z".to_string(),
        progress_pct: Some(50),
        speed_mbps: Some(100.5),
        eta_seconds: Some(120),
        error_message: None,
    };

    let json = serde_json::to_string(&progress).expect("Failed to serialize ModelDownloadProgress");
    assert!(json.contains("qwen-7b"));
    assert!(json.contains("op-12345"));
    assert!(json.contains("50"));

    // Verify JSON structure
    let value: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
    assert_eq!(value["model_id"], "qwen-7b");
    assert_eq!(value["progress_pct"], 50);
    assert_eq!(value["eta_seconds"], 120);
}

/// Test ModelDownloadProgress with error
#[test]
fn test_model_download_progress_with_error() {
    let progress = ModelDownloadProgress {
        model_id: "failed-model".to_string(),
        operation_id: "op-failed".to_string(),
        operation: "import".to_string(),
        status: "failed".to_string(),
        started_at: "2025-01-28T12:00:00Z".to_string(),
        progress_pct: None,
        speed_mbps: None,
        eta_seconds: None,
        error_message: Some("Download failed: connection reset".to_string()),
    };

    let json = serde_json::to_string(&progress).expect("Failed to serialize");
    assert!(json.contains("failed"));
    assert!(json.contains("connection reset"));

    // Verify JSON structure
    let value: serde_json::Value = serde_json::from_str(&json).expect("Invalid JSON");
    assert_eq!(value["status"], "failed");
    assert!(value["error_message"].is_string());
}

/// Test Debug trait implementation for all types
#[test]
fn test_debug_implementation() {
    let request = SeedModelRequest {
        model_name: "debug-test".to_string(),
        model_path: "/test".to_string(),
        format: "mlx".to_string(),
        backend: "mlx".to_string(),
        capabilities: None,
        metadata: None,
    };

    let response = SeedModelResponse {
        import_id: "test".to_string(),
        status: "pending".to_string(),
        message: "Testing".to_string(),
        progress: None,
    };

    let status = ModelStatusResponse {
        model_id: "test".to_string(),
        model_name: "Test".to_string(),
        model_path: None,
        status: ModelLoadStatus::NoModel,
        loaded_at: None,
        error_message: None,
        memory_usage_mb: None,
        is_loaded: false,
        ane_memory: None,
        uma_pressure_level: None,
    };

    // Should not panic - Debug trait is properly derived
    let _ = format!("{:?}", request);
    let _ = format!("{:?}", response);
    let _ = format!("{:?}", status);
}

/// Test various ModelLoadStatus values
#[test]
fn test_model_load_status_variants() {
    let statuses = vec![
        ModelLoadStatus::NoModel,
        ModelLoadStatus::Loading,
        ModelLoadStatus::Ready,
        ModelLoadStatus::Unloading,
        ModelLoadStatus::Error,
    ];

    for status in statuses {
        let response = ModelStatusResponse {
            model_id: format!("{}-model", status.as_str()),
            model_name: "Test".to_string(),
            model_path: None,
            status: status.clone(),
            loaded_at: None,
            error_message: None,
            memory_usage_mb: None,
            is_loaded: status.is_ready(),
            ane_memory: None,
            uma_pressure_level: None,
        };

        let json = serde_json::to_string(&response).expect("Failed to serialize");
        let deserialized: ModelStatusResponse =
            serde_json::from_str(&json).expect("Failed to deserialize");
        assert_eq!(deserialized.status, status);
    }
}
