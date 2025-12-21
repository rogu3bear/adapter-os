//! Tests for FailureCode enum
//!
//! Verifies serialization, deserialization, string conversion,
//! and all enum variants for the structured failure codes.

use adapteros_api_types::failure_code::FailureCode;

#[test]
fn test_all_variants_exist() {
    // Ensure all variants are constructible
    let variants = vec![
        FailureCode::MigrationInvalid,
        FailureCode::ModelLoadFailed,
        FailureCode::OutOfMemory,
        FailureCode::TraceWriteFailed,
        FailureCode::ReceiptMismatch,
        FailureCode::PolicyDivergence,
        FailureCode::BackendFallback,
        FailureCode::TenantAccessDenied,
        FailureCode::KvQuotaExceeded,
        FailureCode::WorkerOverloaded,
        // Boot-specific failure codes
        FailureCode::BootDbUnreachable,
        FailureCode::BootMigrationFailed,
        FailureCode::BootSeedFailed,
        FailureCode::BootNoWorkers,
        FailureCode::BootNoModels,
        FailureCode::BootDependencyTimeout,
        FailureCode::BootBackgroundTaskFailed,
        FailureCode::BootConfigInvalid,
    ];

    assert_eq!(variants.len(), 18, "Expected 18 failure code variants");
}

#[test]
fn test_as_str_all_variants() {
    assert_eq!(FailureCode::MigrationInvalid.as_str(), "MIGRATION_INVALID");
    assert_eq!(FailureCode::ModelLoadFailed.as_str(), "MODEL_LOAD_FAILED");
    assert_eq!(FailureCode::OutOfMemory.as_str(), "OUT_OF_MEMORY");
    assert_eq!(FailureCode::TraceWriteFailed.as_str(), "TRACE_WRITE_FAILED");
    assert_eq!(FailureCode::ReceiptMismatch.as_str(), "RECEIPT_MISMATCH");
    assert_eq!(FailureCode::PolicyDivergence.as_str(), "POLICY_DIVERGENCE");
    assert_eq!(FailureCode::BackendFallback.as_str(), "BACKEND_FALLBACK");
    assert_eq!(
        FailureCode::TenantAccessDenied.as_str(),
        "TENANT_ACCESS_DENIED"
    );
    assert_eq!(FailureCode::KvQuotaExceeded.as_str(), "KV_QUOTA_EXCEEDED");
    assert_eq!(FailureCode::WorkerOverloaded.as_str(), "WORKER_OVERLOADED");
    // Boot-specific failure codes
    assert_eq!(
        FailureCode::BootDbUnreachable.as_str(),
        "BOOT_DB_UNREACHABLE"
    );
    assert_eq!(
        FailureCode::BootMigrationFailed.as_str(),
        "BOOT_MIGRATION_FAILED"
    );
    assert_eq!(FailureCode::BootSeedFailed.as_str(), "BOOT_SEED_FAILED");
    assert_eq!(FailureCode::BootNoWorkers.as_str(), "BOOT_NO_WORKERS");
    assert_eq!(FailureCode::BootNoModels.as_str(), "BOOT_NO_MODELS");
    assert_eq!(
        FailureCode::BootDependencyTimeout.as_str(),
        "BOOT_DEPENDENCY_TIMEOUT"
    );
    assert_eq!(
        FailureCode::BootBackgroundTaskFailed.as_str(),
        "BOOT_BACKGROUND_TASK_FAILED"
    );
    assert_eq!(
        FailureCode::BootConfigInvalid.as_str(),
        "BOOT_CONFIG_INVALID"
    );
}

#[test]
fn test_from_str_all_valid_codes() {
    assert_eq!(
        FailureCode::parse_code("MIGRATION_INVALID"),
        Some(FailureCode::MigrationInvalid)
    );
    assert_eq!(
        FailureCode::parse_code("MODEL_LOAD_FAILED"),
        Some(FailureCode::ModelLoadFailed)
    );
    assert_eq!(
        FailureCode::parse_code("OUT_OF_MEMORY"),
        Some(FailureCode::OutOfMemory)
    );
    assert_eq!(
        FailureCode::parse_code("TRACE_WRITE_FAILED"),
        Some(FailureCode::TraceWriteFailed)
    );
    assert_eq!(
        FailureCode::parse_code("RECEIPT_MISMATCH"),
        Some(FailureCode::ReceiptMismatch)
    );
    assert_eq!(
        FailureCode::parse_code("POLICY_DIVERGENCE"),
        Some(FailureCode::PolicyDivergence)
    );
    assert_eq!(
        FailureCode::parse_code("BACKEND_FALLBACK"),
        Some(FailureCode::BackendFallback)
    );
    assert_eq!(
        FailureCode::parse_code("TENANT_ACCESS_DENIED"),
        Some(FailureCode::TenantAccessDenied)
    );
    assert_eq!(
        FailureCode::parse_code("KV_QUOTA_EXCEEDED"),
        Some(FailureCode::KvQuotaExceeded)
    );
    assert_eq!(
        FailureCode::parse_code("WORKER_OVERLOADED"),
        Some(FailureCode::WorkerOverloaded)
    );
    // Boot-specific failure codes
    assert_eq!(
        FailureCode::parse_code("BOOT_DB_UNREACHABLE"),
        Some(FailureCode::BootDbUnreachable)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_MIGRATION_FAILED"),
        Some(FailureCode::BootMigrationFailed)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_SEED_FAILED"),
        Some(FailureCode::BootSeedFailed)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_NO_WORKERS"),
        Some(FailureCode::BootNoWorkers)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_NO_MODELS"),
        Some(FailureCode::BootNoModels)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_DEPENDENCY_TIMEOUT"),
        Some(FailureCode::BootDependencyTimeout)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_BACKGROUND_TASK_FAILED"),
        Some(FailureCode::BootBackgroundTaskFailed)
    );
    assert_eq!(
        FailureCode::parse_code("BOOT_CONFIG_INVALID"),
        Some(FailureCode::BootConfigInvalid)
    );
}

#[test]
fn test_from_str_invalid_codes() {
    assert_eq!(FailureCode::parse_code("UNKNOWN_CODE"), None);
    assert_eq!(FailureCode::parse_code("migration_invalid"), None); // lowercase
    assert_eq!(FailureCode::parse_code("MigrationInvalid"), None); // PascalCase
    assert_eq!(FailureCode::parse_code(""), None);
    assert_eq!(FailureCode::parse_code("RANDOM_TEXT"), None);
}

#[test]
fn test_from_str_as_str_roundtrip() {
    // Verify that as_str() output can be parsed back via from_str()
    let variants = vec![
        FailureCode::MigrationInvalid,
        FailureCode::ModelLoadFailed,
        FailureCode::OutOfMemory,
        FailureCode::TraceWriteFailed,
        FailureCode::ReceiptMismatch,
        FailureCode::PolicyDivergence,
        FailureCode::BackendFallback,
        FailureCode::TenantAccessDenied,
        FailureCode::KvQuotaExceeded,
        FailureCode::WorkerOverloaded,
        // Boot-specific failure codes
        FailureCode::BootDbUnreachable,
        FailureCode::BootMigrationFailed,
        FailureCode::BootSeedFailed,
        FailureCode::BootNoWorkers,
        FailureCode::BootNoModels,
        FailureCode::BootDependencyTimeout,
        FailureCode::BootBackgroundTaskFailed,
        FailureCode::BootConfigInvalid,
    ];

    for variant in variants {
        let str_repr = variant.as_str();
        let parsed = FailureCode::parse_code(str_repr);
        assert_eq!(parsed, Some(variant), "Roundtrip failed for {:?}", variant);
    }
}

#[test]
fn test_serialize_migration_invalid() {
    let code = FailureCode::MigrationInvalid;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""MIGRATION_INVALID""#);
}

#[test]
fn test_serialize_model_load_failed() {
    let code = FailureCode::ModelLoadFailed;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""MODEL_LOAD_FAILED""#);
}

#[test]
fn test_serialize_out_of_memory() {
    let code = FailureCode::OutOfMemory;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""OUT_OF_MEMORY""#);
}

#[test]
fn test_serialize_trace_write_failed() {
    let code = FailureCode::TraceWriteFailed;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""TRACE_WRITE_FAILED""#);
}

#[test]
fn test_serialize_receipt_mismatch() {
    let code = FailureCode::ReceiptMismatch;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""RECEIPT_MISMATCH""#);
}

#[test]
fn test_serialize_policy_divergence() {
    let code = FailureCode::PolicyDivergence;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""POLICY_DIVERGENCE""#);
}

#[test]
fn test_serialize_backend_fallback() {
    let code = FailureCode::BackendFallback;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""BACKEND_FALLBACK""#);
}

#[test]
fn test_serialize_tenant_access_denied() {
    let code = FailureCode::TenantAccessDenied;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""TENANT_ACCESS_DENIED""#);
}

#[test]
fn test_serialize_kv_quota_exceeded() {
    let code = FailureCode::KvQuotaExceeded;
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""KV_QUOTA_EXCEEDED""#);
}

#[test]
fn test_deserialize_all_variants() {
    let test_cases = vec![
        (r#""MIGRATION_INVALID""#, FailureCode::MigrationInvalid),
        (r#""MODEL_LOAD_FAILED""#, FailureCode::ModelLoadFailed),
        (r#""OUT_OF_MEMORY""#, FailureCode::OutOfMemory),
        (r#""TRACE_WRITE_FAILED""#, FailureCode::TraceWriteFailed),
        (r#""RECEIPT_MISMATCH""#, FailureCode::ReceiptMismatch),
        (r#""POLICY_DIVERGENCE""#, FailureCode::PolicyDivergence),
        (r#""BACKEND_FALLBACK""#, FailureCode::BackendFallback),
        (r#""TENANT_ACCESS_DENIED""#, FailureCode::TenantAccessDenied),
        (r#""KV_QUOTA_EXCEEDED""#, FailureCode::KvQuotaExceeded),
        (r#""WORKER_OVERLOADED""#, FailureCode::WorkerOverloaded),
        // Boot-specific failure codes
        (r#""BOOT_DB_UNREACHABLE""#, FailureCode::BootDbUnreachable),
        (
            r#""BOOT_MIGRATION_FAILED""#,
            FailureCode::BootMigrationFailed,
        ),
        (r#""BOOT_SEED_FAILED""#, FailureCode::BootSeedFailed),
        (r#""BOOT_NO_WORKERS""#, FailureCode::BootNoWorkers),
        (r#""BOOT_NO_MODELS""#, FailureCode::BootNoModels),
        (
            r#""BOOT_DEPENDENCY_TIMEOUT""#,
            FailureCode::BootDependencyTimeout,
        ),
        (
            r#""BOOT_BACKGROUND_TASK_FAILED""#,
            FailureCode::BootBackgroundTaskFailed,
        ),
        (r#""BOOT_CONFIG_INVALID""#, FailureCode::BootConfigInvalid),
    ];

    for (json, expected) in test_cases {
        let parsed: FailureCode = serde_json::from_str(json).unwrap();
        assert_eq!(parsed, expected);
    }
}

#[test]
fn test_deserialize_invalid_json() {
    // Invalid JSON strings should fail to deserialize
    let invalid_cases = vec![
        r#""UNKNOWN_CODE""#,
        r#""migration_invalid""#,
        r#""MigrationInvalid""#,
        r#""""#,
    ];

    for json in invalid_cases {
        let result: Result<FailureCode, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "Expected deserialization to fail for: {}",
            json
        );
    }
}

#[test]
fn test_serialize_deserialize_roundtrip() {
    let variants = vec![
        FailureCode::MigrationInvalid,
        FailureCode::ModelLoadFailed,
        FailureCode::OutOfMemory,
        FailureCode::TraceWriteFailed,
        FailureCode::ReceiptMismatch,
        FailureCode::PolicyDivergence,
        FailureCode::BackendFallback,
        FailureCode::TenantAccessDenied,
        FailureCode::KvQuotaExceeded,
        FailureCode::WorkerOverloaded,
        // Boot-specific failure codes
        FailureCode::BootDbUnreachable,
        FailureCode::BootMigrationFailed,
        FailureCode::BootSeedFailed,
        FailureCode::BootNoWorkers,
        FailureCode::BootNoModels,
        FailureCode::BootDependencyTimeout,
        FailureCode::BootBackgroundTaskFailed,
        FailureCode::BootConfigInvalid,
    ];

    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let parsed: FailureCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, variant, "Roundtrip failed for {:?}", variant);
    }
}

#[test]
fn test_clone_and_copy() {
    let original = FailureCode::OutOfMemory;
    let cloned = original.clone();
    let copied = original;

    assert_eq!(original, cloned);
    assert_eq!(original, copied);
}

#[test]
fn test_debug_format() {
    let code = FailureCode::ModelLoadFailed;
    let debug_str = format!("{:?}", code);
    assert_eq!(debug_str, "ModelLoadFailed");
}

#[test]
fn test_equality() {
    assert_eq!(FailureCode::OutOfMemory, FailureCode::OutOfMemory);
    assert_ne!(FailureCode::OutOfMemory, FailureCode::ModelLoadFailed);
}

#[test]
fn test_screaming_snake_case_serialization() {
    // Verify that serde rename_all = "SCREAMING_SNAKE_CASE" is working
    // by checking the actual JSON output format
    let code = FailureCode::KvQuotaExceeded;
    let json = serde_json::to_string(&code).unwrap();

    // Should be SCREAMING_SNAKE_CASE, not PascalCase or camelCase
    assert!(json.contains("KV_QUOTA_EXCEEDED"));
    assert!(!json.contains("KvQuotaExceeded"));
    assert!(!json.contains("kvQuotaExceeded"));
}

#[test]
fn test_in_json_struct() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct ErrorResponse {
        code: FailureCode,
        message: String,
    }

    let response = ErrorResponse {
        code: FailureCode::TenantAccessDenied,
        message: "Access denied to tenant".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("TENANT_ACCESS_DENIED"));

    let parsed: ErrorResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, response);
    assert_eq!(parsed.code, FailureCode::TenantAccessDenied);
}

#[test]
fn test_in_option() {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Response {
        error_code: Option<FailureCode>,
    }

    // With Some
    let with_error = Response {
        error_code: Some(FailureCode::PolicyDivergence),
    };
    let json = serde_json::to_string(&with_error).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, with_error);

    // With None
    let without_error = Response { error_code: None };
    let json = serde_json::to_string(&without_error).unwrap();
    let parsed: Response = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, without_error);
}

#[test]
fn test_as_str_is_const() {
    // Verify as_str is const and returns static strings
    const CODE_STR: &str = FailureCode::MigrationInvalid.as_str();
    assert_eq!(CODE_STR, "MIGRATION_INVALID");
}
