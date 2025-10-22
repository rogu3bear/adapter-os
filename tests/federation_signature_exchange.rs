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

    println!("✅ Federation signature exchange test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_quorum_verification() -> Result<()> {
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

    println!("✅ Federation quorum verification test passed");

    Ok(())
}

#[tokio::test]
async fn test_federation_output_hash_comparison() -> Result<()> {
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

    println!("✅ Federation output hash comparison test passed");

    Ok(())
}
