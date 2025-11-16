<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

//! Federation signature and output hash integration tests.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{Keypair, Signature as CryptoSignature};
use adapteros_db::Db;
use adapteros_federation::{
    signature::BundleSignatureExchange, ComparisonResult, FederationManager, FederationSignature,
    OutputHashManager,
};
use adapteros_telemetry::StoredBundleMetadata;
use std::fs;
use std::sync::Arc;
use std::time::SystemTime;
use tempfile::TempDir;

fn create_metadata(bundle_root: B3Hash, prev_hash: Option<B3Hash>) -> StoredBundleMetadata {
    StoredBundleMetadata {
        bundle_hash: bundle_root,
        cpid: Some("cpid-001".to_string()),
        tenant_id: "tenant-001".to_string(),
        event_count: 100,
        sequence_no: 42,
        merkle_root: bundle_root,
        signature: String::new(),
        created_at: SystemTime::now(),
        prev_bundle_hash: prev_hash,
        is_incident_bundle: false,
        is_promotion_bundle: false,
        tags: vec![],
    }
}

async fn setup_db(temp_dir: &TempDir, name: &str) -> Result<Db> {
    let path = temp_dir.path().join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| AosError::Io(e.to_string()))?;
    }
    if !path.exists() {
        fs::File::create(&path).map_err(|e| AosError::Io(e.to_string()))?;
    }
    let db_url = format!("sqlite://{}", path.to_string_lossy());
    let db = Db::connect(&db_url)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS federation_bundle_signatures (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            host_id TEXT NOT NULL,
            bundle_hash TEXT NOT NULL,
            signature TEXT NOT NULL,
            prev_host_hash TEXT,
            created_at TEXT,
            verified INTEGER DEFAULT 0
        )
        "#,
    )
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS federation_output_hashes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            host_id TEXT NOT NULL,
            output_hash TEXT NOT NULL,
            input_hash TEXT NOT NULL,
            computed_at TEXT,
            deterministic INTEGER DEFAULT 1
        )
        "#,
    )
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    Ok(db)
}

#[tokio::test]
async fn test_federation_signature_exchange() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();

    let db_a = setup_db(&temp_dir, "host_a.db").await?;
    let db_b = setup_db(&temp_dir, "host_b.db").await?;

    let keypair_a = Keypair::generate();
    let keypair_b = Keypair::generate();
    let pubkey_a = keypair_a.public_key();
    let pubkey_b = keypair_b.public_key();

    let manager_a = FederationManager::with_host_id(db_a.clone(), keypair_a, "host-a".to_string())?;
    let manager_b = FederationManager::with_host_id(db_b.clone(), keypair_b, "host-b".to_string())?;

    let bundle_root = B3Hash::hash(b"bundle-root");
    let metadata = create_metadata(bundle_root, None);

    let signature_a = manager_a.sign_bundle(&metadata).await?;
    let signature_b = manager_b.sign_bundle(&metadata).await?;

    // Each host should accept its own signature
    assert!(manager_a.verify_signature(&signature_a, &pubkey_a, &metadata)?);
    assert!(manager_b.verify_signature(&signature_b, &pubkey_b, &metadata)?);

    // Cross-verification using peer public keys
    assert!(manager_a.verify_signature(&signature_b, &pubkey_b, &metadata)?);
    assert!(manager_b.verify_signature(&signature_a, &pubkey_a, &metadata)?);

    // Chain verification should succeed even when prev_host_hash fields are empty
    manager_a
        .verify_cross_host_chain(&[signature_a.clone(), signature_b.clone()])
        .await?;

    // Validate stored signatures can populate a chain result
    manager_a
        .verify_cross_host_chain(&[signature_a.clone()])
        .await?;
=======
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
>>>>>>> integration-branch

    println!("✅ Federation signature exchange test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_quorum_verification() -> Result<()> {
<<<<<<< HEAD
    let bundle_hash = B3Hash::hash(b"bundle-quorum");
    let mut exchange = BundleSignatureExchange::new(bundle_hash, 2);
    let sig_a = CryptoSignature::from_bytes(&[1u8; 64])?;
    let sig_b = CryptoSignature::from_bytes(&[2u8; 64])?;
    let sig_c = CryptoSignature::from_bytes(&[3u8; 64])?;

    exchange.add_signature("host-a".to_string(), sig_a.clone());
    exchange.add_signature("host-b".to_string(), sig_b.clone());
    assert!(exchange.is_quorum_reached());

    let mut insufficient = BundleSignatureExchange::new(bundle_hash, 4);
    insufficient.add_signature("host-a".to_string(), sig_a);
    insufficient.add_signature("host-b".to_string(), sig_b);
    insufficient.add_signature("host-c".to_string(), sig_c);
    assert!(!insufficient.is_quorum_reached());
=======
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
>>>>>>> integration-branch

    println!("✅ Federation quorum verification test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_output_hash_comparison() -> Result<()> {
<<<<<<< HEAD
    let temp_dir = TempDir::new().unwrap();
    let db = setup_db(&temp_dir, "hash.db").await?;
    let manager = OutputHashManager::new(Arc::new(db.clone()));

    let session_id = "session-1";
    let input_hash = B3Hash::hash(b"input");
    let output_hash = B3Hash::hash(b"output");

    manager
        .record_output_hash(
            session_id.to_string(),
            "host-a".to_string(),
            output_hash,
            input_hash,
            true,
        )
        .await?;
    manager
        .record_output_hash(
            session_id.to_string(),
            "host-b".to_string(),
            output_hash,
            input_hash,
            true,
        )
        .await?;

    let comparison = manager.compare_session(session_id).await?;
    assert!(comparison.is_consistent());

    // Introduce divergence in a new session
    let divergent_hash = B3Hash::hash(b"other-output");
    manager
        .record_output_hash(
            "session-divergent".to_string(),
            "host-a".to_string(),
            output_hash,
            input_hash,
            true,
        )
        .await?;
    manager
        .record_output_hash(
            "session-divergent".to_string(),
            "host-b".to_string(),
            divergent_hash,
            input_hash,
            true,
        )
        .await?;

    let divergence = manager.compare_session("session-divergent").await?;
    assert!(!divergence.is_consistent());
    assert!(matches!(
        divergence,
        ComparisonResult {
            consistent: false,
            ..
        }
    ));
=======
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
>>>>>>> integration-branch

    println!("✅ Federation output hash comparison test passed");

    Ok(())
}
