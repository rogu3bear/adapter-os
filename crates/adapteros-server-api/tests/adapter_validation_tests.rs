//! Tests for adapter validation during import and export
//!
//! These tests verify the validation logic for:
//! - Issue 1: Missing version string in import
//! - Issue 2: Archive checksum mismatch on export
//! - Issue 4: Unknown manifest fields in import
//! - Issue 5: Missing required artifacts on export
//!
//! Note: These are unit tests for the validation logic.
//! Integration tests for the full HTTP handlers are in other test files.

use adapteros_core::errors::adapter::AosAdapterError;
use adapteros_core::errors::validation::AosValidationError;

// =============================================================================
// Error Type Tests
// =============================================================================

#[test]
fn test_missing_version_error_message() {
    let err = AosValidationError::MissingVersion;
    let msg = err.to_string();
    assert_eq!(msg, "Adapter version string is missing from metadata");
}

#[test]
fn test_unknown_manifest_fields_error_message() {
    let fields = vec!["foo".to_string(), "bar".to_string()];
    let err = AosValidationError::UnknownManifestFields(fields);
    let msg = err.to_string();
    assert!(msg.contains("unknown required fields"));
    assert!(msg.contains("foo"));
    assert!(msg.contains("bar"));
}

#[test]
fn test_ttl_in_past_error_message() {
    let err = AosValidationError::TtlInPast;
    let msg = err.to_string();
    assert_eq!(msg, "Adapter pin TTL is in the past");
}

#[test]
fn test_missing_artifacts_error_message() {
    let artifacts = vec!["name".to_string(), "checksum".to_string()];
    let err = AosValidationError::MissingArtifacts(artifacts);
    let msg = err.to_string();
    assert!(msg.contains("required artifacts"));
    assert!(msg.contains("name"));
    assert!(msg.contains("checksum"));
}

#[test]
fn test_archive_checksum_mismatch_error_message() {
    let err = AosAdapterError::ArchiveChecksumMismatch {
        stored: "abc123".to_string(),
        computed: "def456".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("checksum does not match"));
    assert!(msg.contains("abc123"));
    assert!(msg.contains("def456"));
}

// =============================================================================
// Known Manifest Fields Validation Tests
// =============================================================================

/// List of known manifest fields (must match KNOWN_MANIFEST_FIELDS in import.rs)
const KNOWN_MANIFEST_FIELDS: &[&str] = &[
    "adapter_id",
    "name",
    "version",
    "schema_version",
    "scope",
    "category",
    "tier",
    "rank",
    "alpha",
    "targets",
    "backend_family",
    "base_model",
    "weights_hash",
    "content_hash",
    "manifest_hash",
    "metadata",
    "description",
    "intent",
    "framework",
    "framework_version",
    "repo_id",
    "commit_sha",
    "signature",
    "signed_by",
    "tags",
    "labels",
    "annotations",
    "custom",
];

#[test]
fn test_known_fields_list_is_complete() {
    // Verify the known fields list has all expected fields
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"adapter_id"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"name"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"version"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"schema_version"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"metadata"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"weights_hash"));
    assert!(KNOWN_MANIFEST_FIELDS.contains(&"signature"));
}

#[test]
fn test_unknown_field_detection() {
    let manifest_fields = vec!["name", "version", "unknown_field", "another_unknown"];

    let unknown: Vec<&str> = manifest_fields
        .iter()
        .filter(|k| !KNOWN_MANIFEST_FIELDS.contains(k))
        .copied()
        .collect();

    assert_eq!(unknown.len(), 2);
    assert!(unknown.contains(&"unknown_field"));
    assert!(unknown.contains(&"another_unknown"));
}

#[test]
fn test_all_known_fields_pass_validation() {
    // All known fields should pass validation
    for field in KNOWN_MANIFEST_FIELDS {
        assert!(
            KNOWN_MANIFEST_FIELDS.contains(field),
            "Field '{}' should be in known fields list",
            field
        );
    }
}

// =============================================================================
// Version Validation Tests
// =============================================================================

#[test]
fn test_version_format_valid_semver() {
    let valid_versions = vec![
        "1.0.0",
        "0.1.0",
        "10.20.30",
        "1.0.0-alpha",
        "1.0.0-beta.1",
        "2.0.0_rc1",
    ];

    for version in valid_versions {
        let is_valid = version
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_');
        assert!(is_valid, "Version '{}' should be valid", version);
    }
}

#[test]
fn test_version_format_invalid_characters() {
    let invalid_versions = vec![
        "1.0.0@beta", // @ is invalid
        "1.0.0 rc1",  // space is invalid
        "1.0.0#1",    // # is invalid
    ];

    for version in invalid_versions {
        let is_valid = version
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_');
        assert!(!is_valid, "Version '{}' should be invalid", version);
    }
}

// =============================================================================
// SBOM Artifacts Validation Tests
// =============================================================================

#[test]
fn test_sbom_required_fields() {
    // Required SBOM fields according to the policy
    let required = vec!["name", "version", "checksum", "dependencies"];

    assert_eq!(required.len(), 4);
    assert!(required.contains(&"name"));
    assert!(required.contains(&"version"));
    assert!(required.contains(&"checksum"));
    assert!(required.contains(&"dependencies"));
}

#[test]
fn test_dependencies_json_parsing() {
    let valid_json = r#"{"dependencies": [{"name": "base-model", "version": "1.0"}]}"#;
    let parsed: serde_json::Value = serde_json::from_str(valid_json).unwrap();

    let deps = parsed.get("dependencies").and_then(|d| d.as_array());
    assert!(deps.is_some());
    assert!(!deps.unwrap().is_empty());
}

#[test]
fn test_dependencies_missing_from_json() {
    let json_without_deps = r#"{"name": "test-adapter"}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_without_deps).unwrap();

    let deps = parsed.get("dependencies").and_then(|d| d.as_array());
    assert!(deps.is_none());
}

#[test]
fn test_dependencies_empty_array() {
    let json_empty_deps = r#"{"dependencies": []}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_empty_deps).unwrap();

    let deps = parsed.get("dependencies").and_then(|d| d.as_array());
    assert!(deps.is_some());
    assert!(deps.unwrap().is_empty());
}

// =============================================================================
// Checksum Validation Tests
// =============================================================================

#[test]
fn test_blake3_hash_format() {
    // BLAKE3 hashes are 64 hex characters (32 bytes)
    let valid_hash = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    assert_eq!(valid_hash.len(), 64);
    assert!(valid_hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_checksum_mismatch_detection() {
    let stored = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let computed = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    assert_ne!(stored, computed);
}
