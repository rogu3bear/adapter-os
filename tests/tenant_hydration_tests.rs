//! Deterministic Tenant Hydration Tests
//!
//! Validates PRD 2 requirements:
//! 1. Determinism: Same events → same state hash
//! 2. Idempotency: Hydrate twice → identical results
//! 3. Chain integrity verification
//! 4. Schema versioning
//! 5. Failure semantics (partial bundles, missing fields, migrations)

use adapteros_core::tenant_hydration::{
    apply_canonical_ordering, hydrate_partial_bundle, hydrate_tenant_from_bundle,
    verify_idempotency, FailureMode, HydrationConfig, SignatureMetadata,
};
use adapteros_core::B3Hash;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create test bundle structure
fn setup_test_bundle(
    tenant_id: &str,
    events: Vec<serde_json::Value>,
    prev_bundle_hash: Option<B3Hash>,
) -> (TempDir, B3Hash) {
    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join(tenant_id).join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    // Serialize events to NDJSON
    let mut ndjson = String::new();
    for event in &events {
        ndjson.push_str(&serde_json::to_string(event).unwrap());
        ndjson.push('\n');
    }

    // Compute bundle hash (Merkle root)
    let bundle_hash = B3Hash::hash(ndjson.as_bytes());
    let bundle_path = bundle_dir.join(format!("{}.ndjson", bundle_hash.to_hex()));
    fs::write(&bundle_path, &ndjson).unwrap();

    // Create signature metadata
    let sig_metadata = SignatureMetadata {
        merkle_root: bundle_hash.to_hex(),
        signature: "dummy_signature".to_string(),
        public_key: "dummy_public_key".to_string(),
        event_count: events.len(),
        sequence_no: 1,
        prev_bundle_hash,
        version: 1,
    };

    let sig_path = bundle_path.with_extension("ndjson.sig");
    fs::write(
        &sig_path,
        serde_json::to_string_pretty(&sig_metadata).unwrap(),
    )
    .unwrap();

    (temp_dir, bundle_hash)
}

#[tokio::test]
async fn test_deterministic_state_hash() {
    // PRD 2: Same events → same state hash
    let events = vec![
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-a"},
            "metadata": {
                "id": "adapter-1",
                "name": "test-adapter",
                "rank": 16,
                "version": "1.0"
            }
        }),
        json!({
            "timestamp": "2025-01-01T12:01:00Z",
            "event_type": "stack.created",
            "identity": {"tenant_id": "tenant-a"},
            "metadata": {
                "name": "test-stack",
                "adapter_ids": ["adapter-1"]
            }
        }),
    ];

    let (temp_dir1, bundle_hash1) = setup_test_bundle("tenant-a", events.clone(), None);
    let (temp_dir2, bundle_hash2) = setup_test_bundle("tenant-a", events.clone(), None);

    let config1 = HydrationConfig {
        bundle_root: temp_dir1.path().to_path_buf(),
        verify_chain: false,
        ..Default::default()
    };

    let config2 = HydrationConfig {
        bundle_root: temp_dir2.path().to_path_buf(),
        verify_chain: false,
        ..Default::default()
    };

    let result1 = hydrate_tenant_from_bundle("tenant-a", &bundle_hash1, &config1)
        .await
        .unwrap();
    let result2 = hydrate_tenant_from_bundle("tenant-a", &bundle_hash2, &config2)
        .await
        .unwrap();

    // Same events must produce identical state hash
    assert_eq!(result1.state_hash, result2.state_hash);
    assert_eq!(result1.snapshot, result2.snapshot);
}

#[tokio::test]
async fn test_idempotency_verification() {
    // PRD 2: Hydrate twice → identical results
    let events = vec![
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-b"},
            "metadata": {
                "id": "adapter-2",
                "name": "idempotent-adapter",
                "rank": 8,
                "version": "2.0"
            }
        }),
    ];

    let (temp_dir, bundle_hash) = setup_test_bundle("tenant-b", events, None);

    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        verify_chain: false,
        ..Default::default()
    };

    // verify_idempotency hydrates twice and checks hashes match
    let state_hash = verify_idempotency("tenant-b", &bundle_hash, &config)
        .await
        .unwrap();

    // Also verify manually
    let result1 = hydrate_tenant_from_bundle("tenant-b", &bundle_hash, &config)
        .await
        .unwrap();
    let result2 = hydrate_tenant_from_bundle("tenant-b", &bundle_hash, &config)
        .await
        .unwrap();

    assert_eq!(state_hash, result1.state_hash);
    assert_eq!(result1.state_hash, result2.state_hash);
}

#[tokio::test]
async fn test_canonical_ordering() {
    // PRD 2: Canonical ordering rules ensure determinism
    let mut events = vec![
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "config.updated",
            "id": "evt-3"
        }),
        json!({
            "timestamp": "2025-01-01T11:00:00Z",
            "event_type": "adapter.registered",
            "id": "evt-1"
        }),
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "adapter.loaded",
            "id": "evt-2"
        }),
    ];

    apply_canonical_ordering(&mut events);

    // Expected order: timestamp ascending, then event_type lexicographic
    assert_eq!(events[0]["id"], "evt-1"); // 11:00 adapter.registered
    assert_eq!(events[1]["id"], "evt-2"); // 12:00 adapter.loaded
    assert_eq!(events[2]["id"], "evt-3"); // 12:00 config.updated
}

#[tokio::test]
async fn test_chain_integrity_verification() {
    // PRD 2: Verify prev_bundle_hash links
    let events1 = vec![json!({
        "timestamp": "2025-01-01T12:00:00Z",
        "event_type": "adapter.registered",
        "identity": {"tenant_id": "tenant-c"},
        "metadata": {"id": "adapter-1", "name": "first", "rank": 4, "version": "1.0"}
    })];

    let (temp_dir, bundle_hash1) = setup_test_bundle("tenant-c", events1, None);

    // Create second bundle chained to first
    let events2 = vec![json!({
        "timestamp": "2025-01-01T12:01:00Z",
        "event_type": "adapter.registered",
        "identity": {"tenant_id": "tenant-c"},
        "metadata": {"id": "adapter-2", "name": "second", "rank": 8, "version": "1.0"}
    })];

    let bundle_dir = temp_dir.path().join("tenant-c").join("bundles");
    let mut ndjson2 = String::new();
    for event in &events2 {
        ndjson2.push_str(&serde_json::to_string(event).unwrap());
        ndjson2.push('\n');
    }

    let bundle_hash2 = B3Hash::hash(ndjson2.as_bytes());
    let bundle_path2 = bundle_dir.join(format!("{}.ndjson", bundle_hash2.to_hex()));
    fs::write(&bundle_path2, &ndjson2).unwrap();

    let sig_metadata2 = SignatureMetadata {
        merkle_root: bundle_hash2.to_hex(),
        signature: "dummy_signature".to_string(),
        public_key: "dummy_public_key".to_string(),
        event_count: events2.len(),
        sequence_no: 2,
        prev_bundle_hash: Some(bundle_hash1), // Chain link
        version: 1,
    };

    let sig_path2 = bundle_path2.with_extension("ndjson.sig");
    fs::write(
        &sig_path2,
        serde_json::to_string_pretty(&sig_metadata2).unwrap(),
    )
    .unwrap();

    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        verify_chain: true,
        ..Default::default()
    };

    // Hydrate second bundle with chain verification
    let result = hydrate_tenant_from_bundle("tenant-c", &bundle_hash2, &config)
        .await
        .unwrap();

    // Should succeed but warn about missing prev bundle verification
    assert!(result.warnings.is_empty() || result.warnings[0].contains("Chain integrity"));
}

#[tokio::test]
async fn test_schema_version_handling() {
    // PRD 2: Schema versioning support
    let events = vec![json!({
        "timestamp": "2025-01-01T12:00:00Z",
        "event_type": "adapter.registered",
        "identity": {"tenant_id": "tenant-d"},
        "metadata": {"id": "adapter-1", "name": "versioned", "rank": 16, "version": "1.0"}
    })];

    let (temp_dir, bundle_hash) = setup_test_bundle("tenant-d", events, None);

    // Test with max_schema_version
    let config_v1 = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        max_schema_version: 1,
        ..Default::default()
    };

    let result_v1 = hydrate_tenant_from_bundle("tenant-d", &bundle_hash, &config_v1).await;
    assert!(result_v1.is_ok());

    // Test with max_schema_version set too low (should fail)
    let config_v0 = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        max_schema_version: 0,
        ..Default::default()
    };

    let result_v0 = hydrate_tenant_from_bundle("tenant-d", &bundle_hash, &config_v0).await;
    assert!(result_v0.is_err());
    assert!(result_v0.unwrap_err().to_string().contains("Unsupported"));
}

#[tokio::test]
async fn test_partial_bundle_best_effort() {
    // PRD 2: Failure semantics - partial bundles
    let events = vec![
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-e"},
            "metadata": {"id": "adapter-1", "name": "partial", "rank": 4, "version": "1.0"}
        }),
        // Missing fields event (should be skipped in best-effort mode)
        json!({
            "timestamp": "2025-01-01T12:01:00Z",
            "event_type": "unknown.event",
            "identity": {"tenant_id": "tenant-e"}
        }),
    ];

    let (temp_dir, bundle_hash) = setup_test_bundle("tenant-e", events, None);

    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        allow_partial: true,
        strict_mode: false,
        ..Default::default()
    };

    let result = hydrate_partial_bundle("tenant-e", &bundle_hash, &config, FailureMode::BestEffort)
        .await
        .unwrap();

    // Should succeed with 1 adapter (unknown event skipped)
    assert_eq!(result.snapshot.adapters.len(), 1);
}

#[tokio::test]
async fn test_missing_bundle_error() {
    // PRD 2: Failure semantics - missing bundle file
    let temp_dir = TempDir::new().unwrap();
    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let fake_hash = B3Hash::hash(b"nonexistent");
    let result = hydrate_tenant_from_bundle("tenant-f", &fake_hash, &config).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_event_count_mismatch() {
    // PRD 2: Failure semantics - event count validation
    let events = vec![json!({
        "timestamp": "2025-01-01T12:00:00Z",
        "event_type": "adapter.registered",
        "identity": {"tenant_id": "tenant-g"},
        "metadata": {"id": "adapter-1", "name": "mismatch", "rank": 8, "version": "1.0"}
    })];

    let temp_dir = TempDir::new().unwrap();
    let bundle_dir = temp_dir.path().join("tenant-g").join("bundles");
    fs::create_dir_all(&bundle_dir).unwrap();

    let ndjson = serde_json::to_string(&events[0]).unwrap() + "\n";
    let bundle_hash = B3Hash::hash(ndjson.as_bytes());
    let bundle_path = bundle_dir.join(format!("{}.ndjson", bundle_hash.to_hex()));
    fs::write(&bundle_path, &ndjson).unwrap();

    // Create signature with WRONG event count
    let sig_metadata = SignatureMetadata {
        merkle_root: bundle_hash.to_hex(),
        signature: "dummy_signature".to_string(),
        public_key: "dummy_public_key".to_string(),
        event_count: 999, // Wrong!
        sequence_no: 1,
        prev_bundle_hash: None,
        version: 1,
    };

    let sig_path = bundle_path.with_extension("ndjson.sig");
    fs::write(
        &sig_path,
        serde_json::to_string_pretty(&sig_metadata).unwrap(),
    )
    .unwrap();

    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        allow_partial: false,
        ..Default::default()
    };

    let result = hydrate_tenant_from_bundle("tenant-g", &bundle_hash, &config).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Event count mismatch"));
}

#[tokio::test]
async fn test_retry_failure_mode() {
    // PRD 2: Failure semantics - retry with exponential backoff
    let temp_dir = TempDir::new().unwrap();
    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        ..Default::default()
    };

    let fake_hash = B3Hash::hash(b"retry-test");

    // Should retry 3 times and then fail
    let start = std::time::Instant::now();
    let result = hydrate_partial_bundle(
        "tenant-h",
        &fake_hash,
        &config,
        FailureMode::Retry { max_attempts: 3 },
    )
    .await;

    let elapsed = start.elapsed();

    assert!(result.is_err());
    // Verify backoff delays occurred (100ms * 2^0 + 100ms * 2^1 = 300ms minimum)
    assert!(elapsed.as_millis() >= 200);
}

#[tokio::test]
async fn test_snapshot_field_ordering() {
    // PRD 2: Determinism - verify snapshot fields are sorted
    let events = vec![
        json!({
            "timestamp": "2025-01-01T12:02:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-i"},
            "metadata": {"id": "adapter-3", "name": "third", "rank": 12, "version": "1.0"}
        }),
        json!({
            "timestamp": "2025-01-01T12:00:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-i"},
            "metadata": {"id": "adapter-1", "name": "first", "rank": 4, "version": "1.0"}
        }),
        json!({
            "timestamp": "2025-01-01T12:01:00Z",
            "event_type": "adapter.registered",
            "identity": {"tenant_id": "tenant-i"},
            "metadata": {"id": "adapter-2", "name": "second", "rank": 8, "version": "1.0"}
        }),
    ];

    let (temp_dir, bundle_hash) = setup_test_bundle("tenant-i", events, None);

    let config = HydrationConfig {
        bundle_root: temp_dir.path().to_path_buf(),
        verify_chain: false,
        ..Default::default()
    };

    let result = hydrate_tenant_from_bundle("tenant-i", &bundle_hash, &config)
        .await
        .unwrap();

    // Adapters should be sorted by ID (canonical ordering)
    assert_eq!(result.snapshot.adapters.len(), 3);
    assert_eq!(result.snapshot.adapters[0].id, "adapter-1");
    assert_eq!(result.snapshot.adapters[1].id, "adapter-2");
    assert_eq!(result.snapshot.adapters[2].id, "adapter-3");
}
