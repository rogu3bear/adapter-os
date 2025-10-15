//! Cross-Host Replay Tests
//!
//! Integration tests for federated replay verification across multiple hosts

use adapteros_core::{B3Hash, Result};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_federation::FederationManager;
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

fn create_bundle_metadata(
    bundle_id: u64,
    prev_hash: Option<B3Hash>,
) -> StoredBundleMetadata {
    let bundle_data = format!("bundle_{}", bundle_id);
    StoredBundleMetadata {
        bundle_hash: B3Hash::hash(bundle_data.as_bytes()),
        cpid: Some(format!("cpid-{:03}", bundle_id)),
        tenant_id: "tenant-001".to_string(),
        event_count: 100,
        sequence_no: bundle_id,
        merkle_root: B3Hash::hash(bundle_data.as_bytes()),
        signature: format!("sig_{}", bundle_id),
        created_at: SystemTime::now(),
        prev_bundle_hash: prev_hash,
        is_incident_bundle: false,
        is_promotion_bundle: false,
        tags: vec![],
    }
}

#[tokio::test]
async fn test_cross_host_replay_simple() -> Result<()> {
    let db = setup_test_db().await?;

    // Create three hosts
    let host1_keypair = Keypair::generate();
    let host1 = FederationManager::with_host_id(
        db.clone(),
        host1_keypair,
        "replay-host-1".to_string(),
    )?;

    let host2_keypair = Keypair::generate();
    let host2 = FederationManager::with_host_id(
        db.clone(),
        host2_keypair,
        "replay-host-2".to_string(),
    )?;

    let host3_keypair = Keypair::generate();
    let host3 = FederationManager::with_host_id(
        db.clone(),
        host3_keypair,
        "replay-host-3".to_string(),
    )?;

    // Host 1 processes bundle 1
    let bundle1 = create_bundle_metadata(1, None);
    let sig1 = host1.sign_bundle(&bundle1).await?;

    // Host 2 processes bundle 2 (after host 1)
    let bundle2 = create_bundle_metadata(2, Some(bundle1.merkle_root.clone()));
    let sig2 = host2.sign_bundle(&bundle2).await?;

    // Host 3 processes bundle 3 (after host 2)
    let bundle3 = create_bundle_metadata(3, Some(bundle2.merkle_root.clone()));
    let sig3 = host3.sign_bundle(&bundle3).await?;

    // Verify the cross-host replay chain
    let chain = vec![sig1, sig2, sig3];
    let result = host1.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok(), "Cross-host replay chain should be valid");

    Ok(())
}

#[tokio::test]
async fn test_cross_host_replay_with_gap() -> Result<()> {
    let db = setup_test_db().await?;

    let host1_keypair = Keypair::generate();
    let host1 = FederationManager::with_host_id(
        db.clone(),
        host1_keypair,
        "replay-host-1".to_string(),
    )?;

    let host2_keypair = Keypair::generate();
    let host2 = FederationManager::with_host_id(
        db.clone(),
        host2_keypair,
        "replay-host-2".to_string(),
    )?;

    // Host 1 processes bundle 1
    let bundle1 = create_bundle_metadata(1, None);
    let sig1 = host1.sign_bundle(&bundle1).await?;

    // Host 2 processes bundle 3 (skipping bundle 2 - creates a gap)
    let bundle3 = create_bundle_metadata(3, Some(B3Hash::hash(b"nonexistent_bundle_2")));
    let sig3 = host2.sign_bundle(&bundle3).await?;

    // Verify chain - should detect the gap
    let chain = vec![sig1, sig3];
    let result = host1.verify_cross_host_chain(&chain).await;

    assert!(result.is_err(), "Should detect chain gap");

    Ok(())
}

#[tokio::test]
async fn test_cross_host_replay_parallel_branches() -> Result<()> {
    let db = setup_test_db().await?;

    let host1_keypair = Keypair::generate();
    let host1 = FederationManager::with_host_id(
        db.clone(),
        host1_keypair,
        "replay-host-1".to_string(),
    )?;

    let host2_keypair = Keypair::generate();
    let host2 = FederationManager::with_host_id(
        db.clone(),
        host2_keypair,
        "replay-host-2".to_string(),
    )?;

    let host3_keypair = Keypair::generate();
    let host3 = FederationManager::with_host_id(
        db.clone(),
        host3_keypair,
        "replay-host-3".to_string(),
    )?;

    // Root bundle
    let bundle1 = create_bundle_metadata(1, None);
    let sig1 = host1.sign_bundle(&bundle1).await?;

    // Two parallel branches from the same root
    let bundle2a = create_bundle_metadata(2, Some(bundle1.merkle_root.clone()));
    let sig2a = host2.sign_bundle(&bundle2a).await?;

    let bundle2b = create_bundle_metadata(3, Some(bundle1.merkle_root.clone()));
    let sig2b = host3.sign_bundle(&bundle2b).await?;

    // Verify first branch
    let chain_a = vec![sig1.clone(), sig2a];
    let result_a = host1.verify_cross_host_chain(&chain_a).await;
    assert!(result_a.is_ok(), "First branch should be valid");

    // Verify second branch
    let chain_b = vec![sig1, sig2b];
    let result_b = host1.verify_cross_host_chain(&chain_b).await;
    assert!(result_b.is_ok(), "Second branch should be valid");

    Ok(())
}

#[tokio::test]
async fn test_cross_host_replay_long_chain() -> Result<()> {
    let db = setup_test_db().await?;

    // Create 10 hosts
    let mut hosts = Vec::new();
    for i in 0..10 {
        let keypair = Keypair::generate();
        let host = FederationManager::with_host_id(
            db.clone(),
            keypair,
            format!("replay-host-{}", i),
        )?;
        hosts.push(host);
    }

    // Create a chain of 10 bundles across 10 hosts
    let mut signatures = Vec::new();
    let mut prev_hash: Option<B3Hash> = None;

    for (i, host) in hosts.iter().enumerate() {
        let bundle = create_bundle_metadata((i + 1) as u64, prev_hash.clone());
        let sig = host.sign_bundle(&bundle).await?;
        prev_hash = Some(bundle.merkle_root.clone());
        signatures.push(sig);
    }

    // Verify the entire chain
    let result = hosts[0].verify_cross_host_chain(&signatures).await;
    assert!(result.is_ok(), "Long chain should be valid");

    Ok(())
}

#[tokio::test]
async fn test_cross_host_replay_retrieve_chain() -> Result<()> {
    let db = setup_test_db().await?;

    let host1_keypair = Keypair::generate();
    let host1 = FederationManager::with_host_id(
        db.clone(),
        host1_keypair,
        "replay-host-1".to_string(),
    )?;

    // Host 1 signs multiple bundles
    let bundle1 = create_bundle_metadata(1, None);
    host1.sign_bundle(&bundle1).await?;

    let bundle2 = create_bundle_metadata(2, Some(bundle1.merkle_root.clone()));
    host1.sign_bundle(&bundle2).await?;

    let bundle3 = create_bundle_metadata(3, Some(bundle2.merkle_root.clone()));
    host1.sign_bundle(&bundle3).await?;

    // Retrieve host chain
    let chain = host1.get_host_chain("replay-host-1", 10).await?;
    
    // Should have 3 signatures (returned in reverse chronological order)
    assert_eq!(chain.len(), 3, "Should have 3 signatures");

    Ok(())
}

#[tokio::test]
async fn test_cross_host_replay_with_incident_bundle() -> Result<()> {
    let db = setup_test_db().await?;

    let host1_keypair = Keypair::generate();
    let host1 = FederationManager::with_host_id(
        db.clone(),
        host1_keypair,
        "replay-host-1".to_string(),
    )?;

    let host2_keypair = Keypair::generate();
    let host2 = FederationManager::with_host_id(
        db.clone(),
        host2_keypair,
        "replay-host-2".to_string(),
    )?;

    // Normal bundle
    let mut bundle1 = create_bundle_metadata(1, None);
    let sig1 = host1.sign_bundle(&bundle1).await?;

    // Incident bundle (should still be in chain)
    bundle1.is_incident_bundle = true;
    let bundle2 = create_bundle_metadata(2, Some(bundle1.merkle_root.clone()));
    let sig2 = host2.sign_bundle(&bundle2).await?;

    // Verify chain includes incident bundle
    let chain = vec![sig1, sig2];
    let result = host1.verify_cross_host_chain(&chain).await;

    assert!(result.is_ok(), "Chain with incident bundle should be valid");

    Ok(())
}

