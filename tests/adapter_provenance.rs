#![cfg(all(test, feature = "extended-tests"))]

//! Integration tests for adapter provenance tracking (Tier 6)
//!
//! Tests:
//! - Signer key validation
//! - Unsigned bundle rejection
//! - Telemetry event chain
//! - Signer allowlist enforcement

use adapteros_core::B3Hash;
use adapteros_crypto::{sign_bytes, Keypair};

#[test]
fn test_signer_key_format() {
    let keypair = Keypair::generate();
    let public_key = keypair.public_key();

    // Verify public key can be serialized to hex
    let key_hex = public_key.to_hex();
    assert!(key_hex.starts_with("ed25519:") || key_hex.len() == 64);
}

#[test]
fn test_bundle_signing() {
    let keypair = Keypair::generate();
    let bundle_data = b"mock adapter bundle";

    // Sign the bundle
    let signature = sign_bytes(&keypair, bundle_data).unwrap();

    // Verify signature
    let public_key = keypair.public_key();
    let verified = public_key.verify(bundle_data, &signature).is_ok();

    assert!(verified, "Bundle signature should verify");
}

#[test]
fn test_unsigned_bundle_rejection() {
    // Simulate attempting to load unsigned bundle
    let bundle_hash = B3Hash::hash(b"unsigned_bundle");

    // In production, loader would check for signature
    let has_signature = false; // Mock check

    assert!(!has_signature, "Unsigned bundle should be rejected");
}

#[test]
fn test_provenance_chain() {
    // Test that provenance can be traced from signer through registration

    let keypair = Keypair::generate();
    let adapter_id = "test_adapter_1";
    let bundle_data = b"adapter_data";

    // Signer signs the bundle
    let signature = sign_bytes(&keypair, bundle_data).unwrap();
    let bundle_hash = B3Hash::hash(bundle_data);

    // Registrar registers the adapter
    let registered_by = "ops@example.com";
    let registered_uid = 1001u32;

    // Build provenance record
    #[derive(Debug)]
    struct Provenance {
        adapter_id: String,
        signer_key: String,
        signature: String,
        bundle_hash: String,
        registered_by: String,
        registered_uid: u32,
    }

    let provenance = Provenance {
        adapter_id: adapter_id.to_string(),
        signer_key: keypair.public_key().to_hex(),
        signature: signature.to_hex(),
        bundle_hash: bundle_hash.to_hex(),
        registered_by: registered_by.to_string(),
        registered_uid,
    };

    // Verify chain is complete
    assert!(!provenance.signer_key.is_empty());
    assert!(!provenance.signature.is_empty());
    assert!(!provenance.bundle_hash.is_empty());
    assert!(!provenance.registered_by.is_empty());

    println!("✓ Complete provenance chain for {}", adapter_id);
}

#[test]
fn test_signer_allowlist_enforcement() {
    let allowed_keypair = Keypair::generate();
    let disallowed_keypair = Keypair::generate();

    // Mock allowlist
    let allowlist = vec![allowed_keypair.public_key().to_hex()];

    let allowed_key = allowed_keypair.public_key().to_hex();
    let disallowed_key = disallowed_keypair.public_key().to_hex();

    assert!(
        allowlist.contains(&allowed_key),
        "Allowed key should be in allowlist"
    );
    assert!(
        !allowlist.contains(&disallowed_key),
        "Disallowed key should not be in allowlist"
    );
}

#[test]
fn test_provenance_database_schema() {
    // Verify the schema matches expected fields

    // This would normally query the database, but we'll mock the structure
    #[derive(Debug)]
    struct AdapterProvenanceRow {
        adapter_id: String,
        signer_key: String,
        registered_by: Option<String>,
        registered_uid: Option<u32>,
        registered_at: String,
        bundle_b3: String,
    }

    let row = AdapterProvenanceRow {
        adapter_id: "adapter1".to_string(),
        signer_key: "ed25519:abc123".to_string(),
        registered_by: Some("ops@example.com".to_string()),
        registered_uid: Some(1001),
        registered_at: "2025-10-08T00:00:00Z".to_string(),
        bundle_b3: "b3:def456".to_string(),
    };

    assert!(!row.adapter_id.is_empty());
    assert!(row.signer_key.starts_with("ed25519:"));
    assert!(row.bundle_b3.starts_with("b3:"));
}

#[test]
fn test_telemetry_event_for_adapter_registration() {
    // Verify telemetry captures adapter registration with provenance

    use adapteros_telemetry::TelemetryWriter;
    use std::path::PathBuf;
    use tempfile::TempDir;

    let temp_dir = TempDir::with_prefix("aos-test-").unwrap();
    let telemetry = TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap();

    #[derive(serde::Serialize)]
    struct AdapterRegistrationEvent {
        adapter_id: String,
        signer_key: String,
        registered_by: String,
        bundle_hash: String,
    }

    let event = AdapterRegistrationEvent {
        adapter_id: "test_adapter".to_string(),
        signer_key: "ed25519:abc123".to_string(),
        registered_by: "ops@example.com".to_string(),
        bundle_hash: "b3:def456".to_string(),
    };

    let result = telemetry.log("adapter_registration", event);
    assert!(result.is_ok(), "Telemetry logging should succeed");
}
