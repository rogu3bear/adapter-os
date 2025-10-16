//! Federation Chain Tests
//!
//! Tests for cross-host signature chain verification

use adapteros_core::{B3Hash, Result};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_federation::{FederationManager, FederationSignature};
use adapteros_telemetry::StoredBundleMetadata;
use std::time::SystemTime;
use tempfile::TempDir;

async fn setup_test_db() -> Result<Db> {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Db::connect(db_path.to_str().unwrap()).await?;
    db.migrate().await?;
    Ok(db)
}

fn create_test_metadata(bundle_hash: &str, prev_hash: Option<String>) -> StoredBundleMetadata {
    StoredBundleMetadata {
        bundle_hash: B3Hash::hash(bundle_hash.as_bytes()),
        cpid: Some("cpid-001".to_string()),
        tenant_id: "tenant-001".to_string(),
        event_count: 100,
        sequence_no: 1,
        merkle_root: B3Hash::hash(bundle_hash.as_bytes()),
        signature: "test_sig".to_string(),
        created_at: SystemTime::now(),
        prev_bundle_hash: prev_hash.map(|h| B3Hash::hash(h.as_bytes())),
        is_incident_bundle: false,
        is_promotion_bundle: false,
        tags: vec![],
    }
}

#[tokio::test]
async fn test_federation_sign_bundle() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host-1".to_string())?;

    let metadata = create_test_metadata("bundle1", None);
    let signature = manager.sign_bundle(&metadata).await?;

    assert_eq!(signature.host_id, "test-host-1");
    assert_eq!(signature.bundle_hash, metadata.merkle_root.to_string());
    assert!(!signature.signature.is_empty());
    assert_eq!(signature.prev_host_hash, None);

    Ok(())
}

#[tokio::test]
async fn test_federation_verify_valid_chain() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    // Create a valid chain
    let sig1 = FederationSignature::new(
        "host1".to_string(),
        "hash1".to_string(),
        "sig1".to_string(),
        None,
    );

    let sig2 = FederationSignature::new(
        "host2".to_string(),
        "hash2".to_string(),
        "sig2".to_string(),
        Some("hash1".to_string()),
    );

    let sig3 = FederationSignature::new(
        "host3".to_string(),
        "hash3".to_string(),
        "sig3".to_string(),
        Some("hash2".to_string()),
    );

    let chain = vec![sig1, sig2, sig3];
    let result = manager.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_federation_detect_chain_break() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    // Create a chain with a break
    let sig1 = FederationSignature::new(
        "host1".to_string(),
        "hash1".to_string(),
        "sig1".to_string(),
        None,
    );

    let sig2 = FederationSignature::new(
        "host2".to_string(),
        "hash2".to_string(),
        "sig2".to_string(),
        Some("wrong_hash".to_string()), // Chain break here
    );

    let chain = vec![sig1, sig2];
    let result = manager.verify_cross_host_chain(&chain).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("chain break"));

    Ok(())
}

#[tokio::test]
async fn test_federation_empty_chain() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    let chain: Vec<FederationSignature> = vec![];
    let result = manager.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_federation_single_signature() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    let sig1 = FederationSignature::new(
        "host1".to_string(),
        "hash1".to_string(),
        "sig1".to_string(),
        None,
    );

    let chain = vec![sig1];
    let result = manager.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_federation_get_signatures_for_bundle() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    let metadata = create_test_metadata("bundle1", None);
    let signature = manager.sign_bundle(&metadata).await?;

    let signatures = manager
        .get_signatures_for_bundle(&metadata.merkle_root.to_string())
        .await?;

    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].bundle_hash, signature.bundle_hash);

    Ok(())
}

#[tokio::test]
async fn test_federation_mark_verified() -> Result<()> {
    let db = setup_test_db().await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

    let metadata = create_test_metadata("bundle1", None);
    let signature = manager.sign_bundle(&metadata).await?;

    // Initially not verified
    let signatures = manager
        .get_signatures_for_bundle(&metadata.merkle_root.to_string())
        .await?;
    assert!(!signatures[0].verified);

    // Mark as verified
    manager
        .mark_verified(signatures[0].id.as_ref().unwrap())
        .await?;

    // Check it's now verified
    let signatures = manager
        .get_signatures_for_bundle(&metadata.merkle_root.to_string())
        .await?;
    assert!(signatures[0].verified);

    Ok(())
}

#[tokio::test]
async fn test_federation_cross_host_chain() -> Result<()> {
    let db = setup_test_db().await?;

    // Simulate two different hosts
    let keypair1 = Keypair::generate();
    let manager1 = FederationManager::with_host_id(db.clone(), keypair1, "host1".to_string())?;

    let keypair2 = Keypair::generate();
    let manager2 = FederationManager::with_host_id(db.clone(), keypair2, "host2".to_string())?;

    // Host 1 signs bundle 1
    let metadata1 = create_test_metadata("bundle1", None);
    let sig1 = manager1.sign_bundle(&metadata1).await?;

    // Host 2 signs bundle 2 (referencing bundle 1)
    let metadata2 = create_test_metadata("bundle2", Some(sig1.bundle_hash.clone()));
    let sig2 = manager2.sign_bundle(&metadata2).await?;

    // Verify the cross-host chain
    let chain = vec![sig1, sig2];
    let result = manager1.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok());

    Ok(())
}
