//! Test federation signature exchange across hosts
//!
//! This test simulates cross-host bundle signing and verification

use adapteros_core::{B3Hash, Result};
use adapteros_db::Db;
use adapteros_federation::{FederationManager, FederationSignature};
use adapteros_telemetry::TelemetryWriter;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_federation_signature_exchange() -> Result<()> {
    // Setup test databases for two hosts
    let temp_dir = TempDir::new().unwrap();
    let db_a_path = temp_dir.path().join("host_a.db");
    let db_b_path = temp_dir.path().join("host_b.db");

    let db_a = Db::connect(db_a_path.to_str().unwrap()).await?;
    let db_b = Db::connect(db_b_path.to_str().unwrap()).await?;

    db_a.migrate().await?;
    db_b.migrate().await?;

    // Create federation managers for both hosts
    let telemetry_a = TelemetryWriter::new(temp_dir.path().join("telemetry_a"))?;
    let telemetry_b = TelemetryWriter::new(temp_dir.path().join("telemetry_b"))?;

    let federation_a =
        FederationManager::new(Arc::new(db_a), Arc::new(telemetry_a), "host-a".to_string())?;

    let federation_b =
        FederationManager::new(Arc::new(db_b), Arc::new(telemetry_b), "host-b".to_string())?;

    // Register peers
    let peer_info_a = federation_a.get_host_info().await?;
    let peer_info_b = federation_b.get_host_info().await?;

    federation_a.register_peer(peer_info_b.clone()).await?;
    federation_b.register_peer(peer_info_a.clone()).await?;

    // Create a test bundle
    let bundle_hash = B3Hash::new([1u8; 32]);
    let bundle_metadata = adapteros_telemetry::StoredBundleMetadata {
        bundle_hash,
        tenant_id: "test-tenant".to_string(),
        event_count: 100,
        size_bytes: 1024,
        created_at: std::time::SystemTime::now(),
        merkle_root: B3Hash::new([2u8; 32]),
        signature: None,
    };

    // Host A signs the bundle
    let signature_a = federation_a.sign_bundle(&bundle_metadata).await?;
    println!("Host A signed bundle: {}", signature_a.signature);

    // Host B signs the bundle
    let signature_b = federation_b.sign_bundle(&bundle_metadata).await?;
    println!("Host B signed bundle: {}", signature_b.signature);

    // Verify signatures
    let verification_a = federation_a
        .verify_signature(&signature_a, &bundle_metadata)
        .await?;
    let verification_b = federation_b
        .verify_signature(&signature_b, &bundle_metadata)
        .await?;

    assert!(verification_a.is_valid);
    assert!(verification_b.is_valid);

    // Test cross-host verification
    let cross_verification = federation_a
        .verify_signature(&signature_b, &bundle_metadata)
        .await?;
    assert!(cross_verification.is_valid);

    // Test signature chain verification
    let chain_result = federation_a
        .verify_cross_host_chain(vec![signature_a, signature_b])
        .await?;
    assert!(chain_result.is_valid);

    println!("✅ Federation signature exchange test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_quorum_verification() -> Result<()> {
    // Setup test database
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Db::connect(db_path.to_str().unwrap()).await?;
    db.migrate().await?;

    let telemetry = TelemetryWriter::new(temp_dir.path().join("telemetry"))?;
    let federation =
        FederationManager::new(Arc::new(db), Arc::new(telemetry), "test-host".to_string())?;

    // Create test bundle
    let bundle_hash = B3Hash::new([1u8; 32]);
    let bundle_metadata = adapteros_telemetry::StoredBundleMetadata {
        bundle_hash,
        tenant_id: "test-tenant".to_string(),
        event_count: 50,
        size_bytes: 512,
        created_at: std::time::SystemTime::now(),
        merkle_root: B3Hash::new([2u8; 32]),
        signature: None,
    };

    // Create multiple signatures
    let mut signatures = Vec::new();
    for i in 0..3 {
        let host_id = format!("host-{}", i);
        let signature = FederationSignature {
            host_id,
            bundle_hash,
            signature: format!("signature-{}", i),
            prev_host_hash: None,
            created_at: std::time::SystemTime::now(),
            verified: false,
        };
        signatures.push(signature);
    }

    // Test quorum verification (require 2 of 3 signatures)
    let quorum_result = federation.check_quorum(&signatures, 2).await?;
    assert!(quorum_result.is_achieved());

    // Test insufficient quorum (require 4 of 3 signatures)
    let insufficient_result = federation.check_quorum(&signatures, 4).await?;
    assert!(!insufficient_result.is_achieved());

    println!("✅ Federation quorum verification test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_output_hash_comparison() -> Result<()> {
    // Setup test databases for two hosts
    let temp_dir = TempDir::new().unwrap();
    let db_a_path = temp_dir.path().join("host_a.db");
    let db_b_path = temp_dir.path().join("host_b.db");

    let db_a = Db::connect(db_a_path.to_str().unwrap()).await?;
    let db_b = Db::connect(db_b_path.to_str().unwrap()).await?;

    db_a.migrate().await?;
    db_b.migrate().await?;

    let telemetry_a = TelemetryWriter::new(temp_dir.path().join("telemetry_a"))?;
    let telemetry_b = TelemetryWriter::new(temp_dir.path().join("telemetry_b"))?;

    let federation_a =
        FederationManager::new(Arc::new(db_a), Arc::new(telemetry_a), "host-a".to_string())?;

    let federation_b =
        FederationManager::new(Arc::new(db_b), Arc::new(telemetry_b), "host-b".to_string())?;

    // Record output hashes for the same session
    let session_id = "test-session-123";
    let output_hash = B3Hash::new([42u8; 32]);

    federation_a
        .record_output_hash(session_id, &output_hash)
        .await?;
    federation_b
        .record_output_hash(session_id, &output_hash)
        .await?;

    // Compare output hashes
    let comparison = federation_a.compare_output_hashes(session_id).await?;

    match comparison {
        adapteros_federation::ComparisonResult::Match => {
            println!("✅ Output hashes match across hosts");
        }
        adapteros_federation::ComparisonResult::Mismatch(divergences) => {
            panic!(
                "Output hashes should match, but found divergences: {:?}",
                divergences
            );
        }
        adapteros_federation::ComparisonResult::Missing => {
            panic!("Output hashes should be present");
        }
    }

    // Test mismatch scenario
    let different_hash = B3Hash::new([99u8; 32]);
    federation_b
        .record_output_hash("mismatch-session", &different_hash)
        .await?;

    let mismatch_comparison = federation_a
        .compare_output_hashes("mismatch-session")
        .await?;
    match mismatch_comparison {
        adapteros_federation::ComparisonResult::Mismatch(_) => {
            println!("✅ Correctly detected output hash mismatch");
        }
        _ => {
            panic!("Should have detected mismatch");
        }
    }

    println!("✅ Federation output hash comparison test passed");

    Ok(())
}
