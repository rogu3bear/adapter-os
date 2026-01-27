//! Unit tests for adapteros-server-api-models handlers
//!
//! Tests verify placeholder handler behavior and type serialization.

use adapteros_server_api_models::handlers::{
    delete_model, get_model, get_model_status, list_models, register_model, ModelInfo,
    RegisterModelRequest, RegisterModelResponse,
};
use axum::{extract::Path, http::StatusCode, Json};
use chrono::Utc;
use serde_json;
use uuid::Uuid;

/// Test that list_models returns an empty Vec (placeholder behavior)
#[tokio::test]
async fn test_list_models_returns_empty() {
    let result = list_models().await;
    let Json(models) = result;
    assert_eq!(models.len(), 0, "Expected empty model list");
}

/// Test that get_model returns NOT_FOUND (placeholder behavior)
#[tokio::test]
async fn test_get_model_returns_not_found() {
    let model_id = Uuid::new_v4();
    let result = get_model(Path(model_id)).await;
    assert_eq!(
        result,
        Err(StatusCode::NOT_FOUND),
        "Expected NOT_FOUND for placeholder get_model"
    );
}

/// Test that register_model returns NOT_IMPLEMENTED (placeholder behavior)
#[tokio::test]
async fn test_register_model_returns_not_implemented() {
    let request = RegisterModelRequest {
        name: "test-model".to_string(),
        version: "1.0.0".to_string(),
        path: Some("/path/to/model".to_string()),
        metadata: None,
    };
    let result = register_model(Json(request)).await;
    assert_eq!(
        result,
        Err(StatusCode::NOT_IMPLEMENTED),
        "Expected NOT_IMPLEMENTED for placeholder register_model"
    );
}

/// Test that delete_model returns NOT_IMPLEMENTED (placeholder behavior)
#[tokio::test]
async fn test_delete_model_returns_not_implemented() {
    let model_id = Uuid::new_v4();
    let result = delete_model(Path(model_id)).await;
    assert_eq!(
        result,
        StatusCode::NOT_IMPLEMENTED,
        "Expected NOT_IMPLEMENTED for placeholder delete_model"
    );
}

/// Test that get_model_status returns NOT_FOUND (placeholder behavior)
#[tokio::test]
async fn test_get_model_status_returns_not_found() {
    let model_id = Uuid::new_v4();
    let result = get_model_status(Path(model_id)).await;
    assert_eq!(
        result,
        Err(StatusCode::NOT_FOUND),
        "Expected NOT_FOUND for placeholder get_model_status"
    );
}

/// Test ModelInfo serialization and deserialization
#[test]
fn test_model_info_serde() {
    let model_info = ModelInfo {
        id: Uuid::new_v4(),
        name: "Qwen2.5-7B-Instruct".to_string(),
        version: "1.0.0".to_string(),
        status: "ready".to_string(),
        created_at: Utc::now(),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&model_info).expect("Failed to serialize ModelInfo");
    assert!(json.contains("Qwen2.5-7B-Instruct"));
    assert!(json.contains("1.0.0"));
    assert!(json.contains("ready"));

    // Deserialize back
    let deserialized: ModelInfo =
        serde_json::from_str(&json).expect("Failed to deserialize ModelInfo");
    assert_eq!(deserialized.id, model_info.id);
    assert_eq!(deserialized.name, model_info.name);
    assert_eq!(deserialized.version, model_info.version);
    assert_eq!(deserialized.status, model_info.status);
}

/// Test RegisterModelRequest serialization and deserialization
#[test]
fn test_register_model_request_serde() {
    let request = RegisterModelRequest {
        name: "Llama-3-8B".to_string(),
        version: "2.0.0".to_string(),
        path: Some("/var/models/llama-3-8b".to_string()),
        metadata: Some(serde_json::json!({"backend": "mlx", "quantization": "q4"})),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&request).expect("Failed to serialize RegisterModelRequest");
    assert!(json.contains("Llama-3-8B"));
    assert!(json.contains("2.0.0"));
    assert!(json.contains("/var/models/llama-3-8b"));
    assert!(json.contains("mlx"));

    // Deserialize back
    let deserialized: RegisterModelRequest =
        serde_json::from_str(&json).expect("Failed to deserialize RegisterModelRequest");
    assert_eq!(deserialized.name, request.name);
    assert_eq!(deserialized.version, request.version);
    assert_eq!(deserialized.path, request.path);
    assert_eq!(deserialized.metadata, request.metadata);
}

/// Test RegisterModelRequest with minimal fields (optional path and metadata)
#[test]
fn test_register_model_request_minimal_serde() {
    let request = RegisterModelRequest {
        name: "minimal-model".to_string(),
        version: "0.1.0".to_string(),
        path: None,
        metadata: None,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&request).expect("Failed to serialize RegisterModelRequest");
    assert!(json.contains("minimal-model"));
    assert!(json.contains("0.1.0"));

    // Deserialize back
    let deserialized: RegisterModelRequest =
        serde_json::from_str(&json).expect("Failed to deserialize RegisterModelRequest");
    assert_eq!(deserialized.name, request.name);
    assert_eq!(deserialized.version, request.version);
    assert!(deserialized.path.is_none());
    assert!(deserialized.metadata.is_none());
}

/// Test RegisterModelResponse serialization and deserialization
#[test]
fn test_register_model_response_serde() {
    let response = RegisterModelResponse {
        id: Uuid::new_v4(),
        name: "registered-model".to_string(),
        status: "pending".to_string(),
    };

    // Serialize to JSON
    let json =
        serde_json::to_string(&response).expect("Failed to serialize RegisterModelResponse");
    assert!(json.contains("registered-model"));
    assert!(json.contains("pending"));

    // Deserialize back
    let deserialized: RegisterModelResponse =
        serde_json::from_str(&json).expect("Failed to deserialize RegisterModelResponse");
    assert_eq!(deserialized.id, response.id);
    assert_eq!(deserialized.name, response.name);
    assert_eq!(deserialized.status, response.status);
}

/// Test ModelInfo can be constructed properly with valid values
#[test]
fn test_model_info_construction() {
    let model_id = Uuid::new_v4();
    let created_at = Utc::now();

    let model_info = ModelInfo {
        id: model_id,
        name: "Test Model".to_string(),
        version: "1.2.3".to_string(),
        status: "active".to_string(),
        created_at,
    };

    assert_eq!(model_info.id, model_id);
    assert_eq!(model_info.name, "Test Model");
    assert_eq!(model_info.version, "1.2.3");
    assert_eq!(model_info.status, "active");
    assert_eq!(model_info.created_at, created_at);
}

/// Test ModelInfo with different status values
#[test]
fn test_model_info_various_statuses() {
    let statuses = vec!["ready", "pending", "error", "loading", "unloaded"];

    for status in statuses {
        let model_info = ModelInfo {
            id: Uuid::new_v4(),
            name: format!("{}-model", status),
            version: "1.0.0".to_string(),
            status: status.to_string(),
            created_at: Utc::now(),
        };

        assert_eq!(model_info.status, status);
    }
}

/// Test RegisterModelRequest with complex metadata
#[test]
fn test_register_model_request_complex_metadata() {
    let metadata = serde_json::json!({
        "backend": "mlx",
        "quantization": "q4",
        "context_length": 4096,
        "features": ["chat", "completion"],
        "config": {
            "temperature": 0.7,
            "top_p": 0.9
        }
    });

    let request = RegisterModelRequest {
        name: "complex-model".to_string(),
        version: "3.0.0".to_string(),
        path: Some("/models/complex".to_string()),
        metadata: Some(metadata.clone()),
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&request).expect("Failed to serialize");
    let deserialized: RegisterModelRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.metadata, Some(metadata));
}

/// Test that UUID fields serialize correctly
#[test]
fn test_uuid_serialization() {
    let uuid = Uuid::new_v4();
    let model_info = ModelInfo {
        id: uuid,
        name: "uuid-test".to_string(),
        version: "1.0.0".to_string(),
        status: "ready".to_string(),
        created_at: Utc::now(),
    };

    let json = serde_json::to_string(&model_info).expect("Failed to serialize");
    let uuid_str = uuid.to_string();
    assert!(
        json.contains(&uuid_str),
        "JSON should contain UUID as string"
    );
}

/// Test Debug trait implementation for all types
#[test]
fn test_debug_implementation() {
    let model_info = ModelInfo {
        id: Uuid::new_v4(),
        name: "debug-test".to_string(),
        version: "1.0.0".to_string(),
        status: "ready".to_string(),
        created_at: Utc::now(),
    };

    let request = RegisterModelRequest {
        name: "debug-test".to_string(),
        version: "1.0.0".to_string(),
        path: None,
        metadata: None,
    };

    let response = RegisterModelResponse {
        id: Uuid::new_v4(),
        name: "debug-test".to_string(),
        status: "pending".to_string(),
    };

    // Should not panic - Debug trait is properly derived
    let _ = format!("{:?}", model_info);
    let _ = format!("{:?}", request);
    let _ = format!("{:?}", response);
}
