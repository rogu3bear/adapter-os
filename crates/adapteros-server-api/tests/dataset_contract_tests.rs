//! Dataset API Contract Tests
//!
//! Verifies the contract between Rust backend and TypeScript frontend
//! for Dataset endpoints.

use adapteros_api_types::training::{
    DatasetResponse, DatasetValidationStatus, DatasetVersionSummary, DatasetVersionsResponse,
    ValidateDatasetResponse,
};

#[test]
fn test_dataset_validation_status_serialization() {
    let statuses = [
        (DatasetValidationStatus::Pending, "pending"),
        (DatasetValidationStatus::Validating, "validating"),
        (DatasetValidationStatus::Valid, "valid"),
        (DatasetValidationStatus::Invalid, "invalid"),
        (DatasetValidationStatus::Skipped, "skipped"),
    ];

    for (status, expected) in statuses {
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(
            json, expected,
            "Status {:?} should serialize to {}",
            status, expected
        );
    }
}

#[test]
fn test_dataset_validation_status_deserialization() {
    let cases = [
        ("\"pending\"", DatasetValidationStatus::Pending),
        ("\"validating\"", DatasetValidationStatus::Validating),
        ("\"valid\"", DatasetValidationStatus::Valid),
        ("\"invalid\"", DatasetValidationStatus::Invalid),
        ("\"skipped\"", DatasetValidationStatus::Skipped),
    ];

    for (json_str, expected) in cases {
        let status: DatasetValidationStatus = serde_json::from_str(json_str).unwrap();
        assert_eq!(status, expected);
    }
}

#[test]
fn test_dataset_response_required_fields() {
    let response = DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: "ds-123".to_string(),
        dataset_version_id: Some("ds-ver-001".to_string()),
        name: "Test Dataset".to_string(),
        description: Some("A test dataset".to_string()),
        file_count: 5,
        total_size_bytes: 1024000,
        format: "jsonl".to_string(),
        hash: "abc123def456".to_string(),
        storage_path: "/var/data/ds-123".to_string(),
        validation_status: DatasetValidationStatus::Valid,
        validation_errors: None,
        trust_state: Some("allowed".to_string()),
        created_by: "user@test.com".to_string(),
        created_at: "2025-01-15T00:00:00Z".to_string(),
        updated_at: "2025-01-15T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&response).unwrap();

    // Required fields must be present and correctly typed
    assert!(json["dataset_id"].is_string());
    assert!(json["dataset_version_id"].is_string());
    assert!(json["name"].is_string());
    assert!(json["file_count"].is_i64());
    assert!(json["total_size_bytes"].is_i64());
    assert!(json["validation_status"].is_string());
    assert_eq!(json["validation_status"], "valid");
    assert_eq!(json["trust_state"], "allowed");

    // Optional fields must be null or correct type when present
    assert!(json["description"].is_string());
}

#[test]
fn test_validation_errors_is_array() {
    let response = DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: "ds-123".to_string(),
        dataset_version_id: Some("ds-ver-001".to_string()),
        name: "Test".to_string(),
        description: None,
        file_count: 1,
        total_size_bytes: 100,
        format: "jsonl".to_string(),
        hash: "abc123".to_string(),
        storage_path: "var/test-datasets".to_string(),
        validation_status: DatasetValidationStatus::Invalid,
        validation_errors: Some(vec![
            "File data.jsonl: Invalid JSON at line 5".to_string(),
            "File config.json: Missing required field".to_string(),
        ]),
        trust_state: Some("blocked".to_string()),
        created_by: "test".to_string(),
        created_at: "2025-01-15T00:00:00Z".to_string(),
        updated_at: "2025-01-15T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&response).unwrap();

    assert!(json["validation_errors"].is_array());
    let errors = json["validation_errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert!(errors[0].is_string());
}

#[test]
fn test_validation_errors_null_when_none() {
    let response = DatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: "ds-123".to_string(),
        dataset_version_id: None,
        name: "Test".to_string(),
        description: None,
        file_count: 1,
        total_size_bytes: 100,
        format: "jsonl".to_string(),
        hash: "abc123".to_string(),
        storage_path: "var/test-datasets".to_string(),
        validation_status: DatasetValidationStatus::Valid,
        validation_errors: None,
        trust_state: None,
        created_by: "test".to_string(),
        created_at: "2025-01-15T00:00:00Z".to_string(),
        updated_at: "2025-01-15T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&response).unwrap();

    // With skip_serializing_if = "Option::is_none", field should be absent
    assert!(json.get("validation_errors").is_none() || json["validation_errors"].is_null());
}

#[test]
fn test_validate_dataset_response_contract() {
    let response = ValidateDatasetResponse {
        schema_version: "1.0".to_string(),
        dataset_id: "ds-123".to_string(),
        is_valid: false,
        validation_status: DatasetValidationStatus::Invalid,
        errors: Some(vec!["Error 1".to_string(), "Error 2".to_string()]),
        validated_at: "2025-01-15T00:00:00Z".to_string(),
    };

    let json = serde_json::to_value(&response).unwrap();

    assert!(json["is_valid"].is_boolean());
    assert_eq!(json["is_valid"], false);
    assert_eq!(json["validation_status"], "invalid");
    assert!(json["errors"].is_array());
}

#[test]
fn test_dataset_validation_status_from_db_string() {
    assert_eq!(
        DatasetValidationStatus::from_db_string("pending"),
        DatasetValidationStatus::Pending
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("draft"),
        DatasetValidationStatus::Pending
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("validating"),
        DatasetValidationStatus::Validating
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("valid"),
        DatasetValidationStatus::Valid
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("invalid"),
        DatasetValidationStatus::Invalid
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("skipped"),
        DatasetValidationStatus::Skipped
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("failed"),
        DatasetValidationStatus::Pending
    );
    assert_eq!(
        DatasetValidationStatus::from_db_string("unknown"),
        DatasetValidationStatus::Pending
    );
}

#[test]
fn test_dataset_versions_response_contract() {
    let response = DatasetVersionsResponse {
        schema_version: "1.0".to_string(),
        dataset_id: "ds-123".to_string(),
        versions: vec![DatasetVersionSummary {
            dataset_version_id: "ver-1".to_string(),
            version_number: 1,
            version_label: Some("v1".to_string()),
            hash_b3: Some("hash-v1".to_string()),
            storage_path: Some("var/test-datasets/ver1".to_string()),
            trust_state: Some("allowed".to_string()),
            created_at: "2025-02-01T00:00:00Z".to_string(),
        }],
    };

    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["dataset_id"], "ds-123");
    assert!(json["versions"].is_array());
    assert_eq!(json["versions"][0]["trust_state"], "allowed");
    assert_eq!(json["versions"][0]["version_number"], 1);
}
