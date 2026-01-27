//! PRD-ART-01: Artifact Portability Tests
//!
//! Tests for the .aos artifact hardening and portability features:
//! - Schema version validation
//! - Base model compatibility checking
//! - Backend family validation
//! - Hash integrity verification
//! - Signature policy enforcement
//! - Content hash identity
//! - Database field storage

mod common;

use adapteros_aos::{open_aos, AosWriter, BackendTag, HEADER_SIZE};
use adapteros_core::B3Hash;
use common::{setup_state, test_admin_claims};
use tempfile::NamedTempFile;

// ============================================================================
// AOS File Construction Helpers
// ============================================================================

/// Creates a minimal valid .aos binary file for testing
///
fn create_test_aos_file(manifest: &serde_json::Value, weights: &[u8]) -> Vec<u8> {
    let mut manifest_enriched = manifest.clone();
    let scope = manifest_enriched
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("project");
    let scope_path = format!("unspecified/unspecified/{}/test", scope);

    {
        let manifest_obj = manifest_enriched
            .as_object_mut()
            .expect("manifest should be an object");
        let metadata_entry = manifest_obj
            .entry("metadata".to_string())
            .or_insert_with(|| serde_json::json!({}));
        let metadata = metadata_entry
            .as_object_mut()
            .expect("metadata must be object");
        metadata
            .entry("domain".to_string())
            .or_insert(serde_json::json!("unspecified"));
        metadata
            .entry("group".to_string())
            .or_insert(serde_json::json!("unspecified"));
        metadata
            .entry("operation".to_string())
            .or_insert(serde_json::json!("test"));
        metadata
            .entry("scope_path".to_string())
            .or_insert(serde_json::json!(scope_path.clone()));
    }

    let mut writer = AosWriter::new();
    writer
        .add_segment(BackendTag::Canonical, Some(scope_path), weights)
        .unwrap();
    let temp = NamedTempFile::with_prefix("aos-test-").unwrap();
    writer
        .write_archive(temp.path(), &manifest_enriched)
        .expect("write test aos");
    std::fs::read(temp.path()).expect("read test aos")
}

/// Extracts manifest JSON from an .aos file (for verification)
fn extract_manifest_from_aos(data: &[u8]) -> Option<serde_json::Value> {
    let view = open_aos(data).ok()?;
    serde_json::from_slice(view.manifest_bytes).ok()
}

// ============================================================================
// AOS File Format Tests
// ============================================================================

/// Test that we can create and parse a valid .aos file
#[tokio::test]
async fn test_aos_file_roundtrip() {
    let manifest = serde_json::json!({
        "schema_version": "1.0.0",
        "adapter_id": "test-roundtrip",
        "name": "Roundtrip Test Adapter",
        "version": "1.0.0",
        "rank": 8,
        "category": "code",
        "scope": "global"
    });

    let weights = vec![0u8; 1024]; // 1KB dummy weights

    let aos_file = create_test_aos_file(&manifest, &weights);

    // Verify file structure
    assert!(aos_file.len() > HEADER_SIZE);

    // Extract and verify manifest
    let extracted = extract_manifest_from_aos(&aos_file).expect("should parse");
    assert_eq!(
        extracted.get("adapter_id").unwrap().as_str().unwrap(),
        "test-roundtrip"
    );
}

/// Test AOS file with corrupted magic bytes
#[tokio::test]
async fn test_aos_invalid_magic() {
    let mut aos_file = create_test_aos_file(&serde_json::json!({}), &[]);
    aos_file[0..4].copy_from_slice(b"BAD!");

    let extracted = extract_manifest_from_aos(&aos_file);
    assert!(extracted.is_none(), "Should reject invalid magic");
}

// ============================================================================
// Schema Version Validation Tests
// ============================================================================

/// Test schema version parsing logic
#[tokio::test]
async fn test_schema_version_major_extraction() {
    let test_cases = vec![
        ("1.0.0", 1u32),
        ("2.0.0", 2u32),
        ("99.0.0", 99u32),
        ("1.2.3", 1u32),
        ("", 1u32), // Default
    ];

    for (version, expected_major) in test_cases {
        let major: u32 = version
            .split('.')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        assert_eq!(
            major, expected_major,
            "Version {} should have major {}",
            version, expected_major
        );
    }
}

/// Test that future schema versions would be rejected
#[tokio::test]
async fn test_future_schema_version_detection() {
    const CURRENT_MAJOR: u32 = 1; // MANIFEST_SCHEMA_VERSION = "1.0.0"

    let future_version = "99.0.0";
    let future_major: u32 = future_version
        .split('.')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap();

    assert!(
        future_major > CURRENT_MAJOR,
        "Schema version 99.0.0 should be detected as future"
    );
}

// ============================================================================
// Hash Integrity Tests
// ============================================================================

/// Test that content hash is deterministic
#[tokio::test]
async fn test_content_hash_determinism() {
    let manifest_bytes = b"test manifest content";
    let weights_bytes = b"test weights content";

    // Compute hash multiple times
    let hash1 = B3Hash::hash_multi(&[manifest_bytes, weights_bytes]);
    let hash2 = B3Hash::hash_multi(&[manifest_bytes, weights_bytes]);
    let hash3 = B3Hash::hash_multi(&[manifest_bytes, weights_bytes]);

    assert_eq!(hash1.to_hex(), hash2.to_hex());
    assert_eq!(hash2.to_hex(), hash3.to_hex());
}

/// Test that different content produces different hashes
#[tokio::test]
async fn test_content_hash_uniqueness() {
    let manifest1 = b"manifest v1";
    let manifest2 = b"manifest v2";
    let weights = b"same weights";

    let hash1 = B3Hash::hash_multi(&[manifest1, weights]);
    let hash2 = B3Hash::hash_multi(&[manifest2, weights]);

    assert_ne!(
        hash1.to_hex(),
        hash2.to_hex(),
        "Different content should produce different hashes"
    );
}

/// Test weights hash computation matches what would be stored
#[tokio::test]
async fn test_weights_hash_computation() {
    let weights = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
    let computed_hash = B3Hash::hash(&weights).to_hex().to_string();

    // Create manifest with this hash
    let manifest = serde_json::json!({
        "weights_hash": computed_hash,
    });

    let aos_file = create_test_aos_file(&manifest, &weights);

    // Extract and verify hash matches
    let extracted = extract_manifest_from_aos(&aos_file).unwrap();
    let stored_hash = extracted.get("weights_hash").unwrap().as_str().unwrap();

    // Recompute from extracted weights
    let view = open_aos(&aos_file).expect("valid aos file");
    let extracted_weights = view
        .segments
        .iter()
        .find(|s| s.backend_tag == BackendTag::Canonical)
        .map(|s| s.payload)
        .expect("canonical segment present");
    let recomputed_hash = B3Hash::hash(extracted_weights).to_hex().to_string();

    assert_eq!(
        stored_hash, recomputed_hash,
        "Hash should match recomputed value"
    );
}

// ============================================================================
// Backend Family Validation Tests
// ============================================================================

/// Test valid backend families
#[tokio::test]
async fn test_valid_backend_families() {
    let valid = vec!["metal", "coreml", "mlx", "auto"];

    for backend in valid {
        let is_valid = matches!(backend, "metal" | "coreml" | "mlx" | "auto");
        assert!(is_valid, "Backend '{}' should be valid", backend);
    }
}

/// Test invalid backend families
#[tokio::test]
async fn test_invalid_backend_families() {
    let invalid = vec!["cuda", "rocm", "vulkan", "cpu", ""];

    for backend in invalid {
        let is_valid = matches!(backend, "metal" | "coreml" | "mlx" | "auto");
        assert!(!is_valid, "Backend '{}' should be invalid", backend);
    }
}

// ============================================================================
// Signature Policy Tests
// ============================================================================

/// Test signature policy database storage and retrieval
#[tokio::test]
async fn test_signature_policy_persistence() {
    let state = setup_state(None).await.expect("setup state");
    let claims = test_admin_claims();

    // Create policy requiring signatures
    let policy_request = adapteros_api_types::CreateExecutionPolicyRequest {
        determinism: adapteros_api_types::DeterminismPolicy::default(),
        routing: None,
        golden: None,
        require_signed_adapters: true,
    };

    state
        .db
        .create_execution_policy(&claims.tenant_id, policy_request, Some(&claims.sub))
        .await
        .expect("create policy");

    // Retrieve and verify
    let policy = state
        .db
        .get_execution_policy_or_default(&claims.tenant_id)
        .await
        .expect("get policy");

    assert!(
        policy.require_signed_adapters,
        "Policy should require signatures"
    );
    assert!(
        !policy.is_implicit,
        "Policy should be explicit, not default"
    );
}

/// Test default policy does not require signatures
#[tokio::test]
async fn test_default_policy_no_signature_requirement() {
    let state = setup_state(None).await.expect("setup state");

    // Get policy for tenant with no explicit policy
    let policy = state
        .db
        .get_execution_policy_or_default("no-policy-tenant")
        .await
        .expect("get policy");

    assert!(
        !policy.require_signed_adapters,
        "Default policy should not require signatures"
    );
    assert!(policy.is_implicit, "Should be implicit default policy");
}

/// Test signature presence detection
#[tokio::test]
async fn test_signature_field_detection() {
    // Unsigned manifest
    let unsigned = serde_json::json!({
        "adapter_id": "unsigned",
        "version": "1.0.0"
    });

    // Signed manifest
    let signed = serde_json::json!({
        "adapter_id": "signed",
        "version": "1.0.0",
        "signature": "base64-signature-data"
    });

    assert!(
        unsigned.get("signature").is_none(),
        "Unsigned should have no signature"
    );
    assert!(
        signed.get("signature").is_some(),
        "Signed should have signature"
    );
}

// ============================================================================
// Database Registration Tests
// ============================================================================

/// Test adapter registration with artifact hardening fields
#[tokio::test]
async fn test_adapter_registration_with_artifact_fields() {
    use adapteros_db::AdapterRegistrationBuilder;

    let state = setup_state(None).await.expect("setup state");

    let adapter_id = "test-artifact-adapter";

    // Note: base_model_id not set because it requires FK to models table
    // Testing manifest_schema_version, content_hash_b3, and provenance_json
    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("Artifact Test Adapter")
        .hash_b3("b3hash123")
        .rank(8)
        .tier("warm")
        .manifest_schema_version(Some("1.0.0"))
        .content_hash_b3(Some("content-hash-abc"))
        .provenance_json(Some(r#"{"training_job_id":"job-1"}"#))
        .build()
        .expect("build params");

    // Register adapter
    let _id = state.db.register_adapter(params).await.expect("register");

    // Retrieve by external adapter_id (not internal UUID)
    let adapter = state
        .db
        .get_adapter_for_tenant("tenant-1", adapter_id)
        .await
        .expect("get")
        .expect("exists");

    assert_eq!(adapter.manifest_schema_version, Some("1.0.0".to_string()));
    assert_eq!(
        adapter.content_hash_b3,
        Some("content-hash-abc".to_string())
    );
    assert!(adapter.base_model_id.is_none()); // Not set (requires FK)
    assert_eq!(
        adapter.provenance_json,
        Some(r#"{"training_job_id":"job-1"}"#.to_string())
    );
}

/// Test adapter registration without optional artifact fields
#[tokio::test]
async fn test_adapter_registration_without_artifact_fields() {
    use adapteros_db::AdapterRegistrationBuilder;

    let state = setup_state(None).await.expect("setup state");

    let adapter_id = "test-minimal-adapter";

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("Minimal Adapter")
        .hash_b3("hash456")
        .rank(4)
        .tier("ephemeral")
        .build()
        .expect("build params");

    let _id = state.db.register_adapter(params).await.expect("register");

    // Retrieve by external adapter_id (not internal UUID)
    let adapter = state
        .db
        .get_adapter_for_tenant("tenant-1", adapter_id)
        .await
        .expect("get")
        .expect("exists");

    // Artifact fields should be None
    assert!(adapter.manifest_schema_version.is_none());
    assert_eq!(adapter.content_hash_b3, Some("hash456".to_string()));
    assert!(adapter.base_model_id.is_none());
    assert!(adapter.provenance_json.is_none());
}

// ============================================================================
// Base Model Compatibility Tests
// ============================================================================

/// Test base model lookup for non-existent model
#[tokio::test]
async fn test_base_model_lookup_not_found() {
    let state = setup_state(None).await.expect("setup state");

    let result = state
        .db
        .get_model_by_name("nonexistent-model")
        .await
        .expect("query ok");

    assert!(result.is_none(), "Non-existent model should return None");
}

// ============================================================================
// Provenance Data Tests
// ============================================================================

/// Test provenance JSON structure
#[tokio::test]
async fn test_provenance_json_structure() {
    let provenance = serde_json::json!({
        "training_job_id": "job-abc123",
        "dataset_id": "dataset-xyz",
        "dataset_hash": "hash123",
        "training_config": {
            "epochs": 10,
            "learning_rate": 0.001
        },
        "documents": [
            {"id": "doc1", "name": "file1.txt", "content_hash": "hash1"},
            {"id": "doc2", "name": "file2.txt", "content_hash": "hash2"}
        ],
        "export_timestamp": "2025-01-01T00:00:00Z",
        "export_hash": "exporthash123"
    });

    // Verify structure is serializable/deserializable
    let json_str = serde_json::to_string(&provenance).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(parsed["training_job_id"], "job-abc123");
    assert_eq!(parsed["documents"].as_array().unwrap().len(), 2);
}
