//! Signature Chain Tests - Comprehensive tests for federation signature chain validation
//!
//! Tests the signature chain validation logic including:
//! - Chain validation with missing hosts
//! - Signature verification timeouts
//! - Cross-host clock skew handling
//! - Chain continuity verification (prev_hash linking)
//! - Invalid signature detection

use adapteros_core::{B3Hash, Result};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_federation::{
    signature::{BundleSignatureExchange, QuorumManager},
    FederationManager, FederationSignature,
};
use adapteros_telemetry::StoredBundleMetadata;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

// Helper to setup a test database with proper cleanup
// Returns (Db, TempDir) - caller must hold TempDir to keep DB files alive
async fn setup_test_db() -> Result<(Db, TempDir)> {
    let temp_dir = TempDir::with_prefix("aos-federation-test-")
        .expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join(format!("test_{}.db", uuid::Uuid::new_v4()));
    let db = Db::connect(db_path.to_str().unwrap()).await?;
    db.migrate().await?;
    Ok((db, temp_dir))
}

// Helper to create test bundle metadata
fn create_test_metadata(
    merkle_root: &str,
    prev_bundle_hash: Option<String>,
) -> StoredBundleMetadata {
    StoredBundleMetadata {
        bundle_hash: B3Hash::hash(b"test_bundle"),
        cpid: Some("cpid-001".to_string()),
        tenant_id: Some("tenant-001".to_string()),
        event_count: 100,
        sequence_no: Some(1),
        merkle_root: B3Hash::hash(merkle_root.as_bytes()),
        signature: "test_sig".to_string(),
        public_key: "test_pubkey".to_string(),
        key_id: "test_key_id".to_string(),
        schema_version: 1,
        signed_at_us: 0,
        created_at: std::time::SystemTime::now(),
        prev_bundle_hash: prev_bundle_hash.map(|h| B3Hash::hash(h.as_bytes())),
        is_incident_bundle: false,
        is_promotion_bundle: false,
        tags: vec![],
        stack_id: None,
        stack_version: None,
    }
}

// Helper to create a FederationSignature with specific timestamp
fn create_signature_with_time(
    host_id: &str,
    bundle_hash: &str,
    signature: &str,
    prev_host_hash: Option<String>,
    created_at: DateTime<Utc>,
) -> FederationSignature {
    FederationSignature {
        id: Some(uuid::Uuid::new_v4().to_string()),
        host_id: host_id.to_string(),
        bundle_hash: bundle_hash.to_string(),
        signature: signature.to_string(),
        prev_host_hash,
        created_at,
        verified: false,
    }
}

// ============================================================================
// Chain Validation with Missing Hosts Tests
// ============================================================================

mod chain_validation_missing_hosts {
    use super::*;

    #[tokio::test]
    async fn test_empty_chain_is_valid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        // Empty chain should be valid
        let empty_chain: Vec<FederationSignature> = vec![];
        let result = manager.verify_cross_host_chain(&empty_chain).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_single_signature_chain_is_valid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        // Single signature chain should be valid (no linkage check needed)
        let single_sig = FederationSignature::new(
            "host1".to_string(),
            "hash1".to_string(),
            "sig1".to_string(),
            None,
        );

        let chain = vec![single_sig];
        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_chain_with_gap_in_hosts() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        // Chain where host2 is missing but links are correct
        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        // Host 3 links directly to host 1 (host 2 is missing from chain)
        let sig3 = create_signature_with_time(
            "host3",
            "hash3",
            "sig3",
            Some("hash1".to_string()),
            now + Duration::seconds(2),
        );

        let chain = vec![sig1, sig3];
        let result = manager.verify_cross_host_chain(&chain).await;

        // Chain should be valid since prev_hash links correctly
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_chain_with_missing_prev_hash_warning() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        // Signature 2 has no prev_hash at all (should log warning but not fail)
        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            None, // Missing prev_hash
            now + Duration::seconds(1),
        );

        let chain = vec![sig1, sig2];

        // This should complete without error but would log a warning
        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_ok());

        Ok(())
    }
}

// ============================================================================
// Cross-Host Clock Skew Tests
// ============================================================================

mod clock_skew_handling {
    use super::*;

    #[tokio::test]
    async fn test_future_timestamp_in_chain_fails() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        // First signature is in the future relative to the second
        let sig1 = create_signature_with_time(
            "host1",
            "hash1",
            "sig1",
            None,
            now + Duration::hours(1), // In the future
        );

        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("hash1".to_string()),
            now, // In the past relative to sig1
        );

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;

        // Should fail due to timestamp violation
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timestamp violation"));

        Ok(())
    }

    #[tokio::test]
    async fn test_past_timestamp_within_tolerance() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        // Signatures with proper timestamp ordering
        let sig1 =
            create_signature_with_time("host1", "hash1", "sig1", None, now - Duration::hours(1));

        let sig2 =
            create_signature_with_time("host2", "hash2", "sig2", Some("hash1".to_string()), now);

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;

        // Should succeed with proper ordering
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_same_timestamp_is_valid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        // Two signatures with exactly the same timestamp (edge case)
        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("hash1".to_string()),
            now, // Same timestamp
        );

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;

        // Same timestamp should be valid (not less than)
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_millisecond_difference_ordering() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        // Signatures with millisecond differences
        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("hash1".to_string()),
            now + Duration::milliseconds(1),
        );

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_ok());

        Ok(())
    }
}

// ============================================================================
// Chain Continuity Verification (prev_hash linking) Tests
// ============================================================================

mod chain_continuity {
    use super::*;

    #[tokio::test]
    async fn test_valid_chain_linkage() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("hash1".to_string()), // Links to sig1.bundle_hash
            now + Duration::seconds(1),
        );

        let sig3 = create_signature_with_time(
            "host3",
            "hash3",
            "sig3",
            Some("hash2".to_string()), // Links to sig2.bundle_hash
            now + Duration::seconds(2),
        );

        let chain = vec![sig1, sig2, sig3];
        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_chain_break_detection() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        // sig2 has wrong prev_hash - chain break!
        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("wrong_hash".to_string()), // Should be "hash1"
            now + Duration::seconds(1),
        );

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("chain break"));

        Ok(())
    }

    #[tokio::test]
    async fn test_long_chain_validation() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();
        let mut chain = Vec::new();
        let mut prev_hash: Option<String> = None;

        // Create a chain of 10 signatures
        for i in 0..10 {
            let hash = format!("hash{}", i);
            let sig = create_signature_with_time(
                &format!("host{}", i),
                &hash,
                &format!("sig{}", i),
                prev_hash.clone(),
                now + Duration::seconds(i as i64),
            );
            chain.push(sig);
            prev_hash = Some(hash);
        }

        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_chain_break_in_middle() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let now = Utc::now();

        let sig1 = create_signature_with_time("host1", "hash1", "sig1", None, now);

        let sig2 = create_signature_with_time(
            "host2",
            "hash2",
            "sig2",
            Some("hash1".to_string()),
            now + Duration::seconds(1),
        );

        // Chain break at position 3
        let sig3 = create_signature_with_time(
            "host3",
            "hash3",
            "sig3",
            Some("bad_link".to_string()), // Should be "hash2"
            now + Duration::seconds(2),
        );

        let sig4 = create_signature_with_time(
            "host4",
            "hash4",
            "sig4",
            Some("hash3".to_string()),
            now + Duration::seconds(3),
        );

        let chain = vec![sig1, sig2, sig3, sig4];
        let result = manager.verify_cross_host_chain(&chain).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("host2"));
        assert!(err.to_string().contains("host3"));

        Ok(())
    }
}

// ============================================================================
// Invalid Signature Detection Tests
// ============================================================================

mod invalid_signature_detection {
    use super::*;

    #[tokio::test]
    async fn test_verify_valid_signature() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        // Create signature with the keypair
        let payload = serde_json::to_vec(&metadata)?;
        let signature = keypair.sign(&payload);
        let signature_hex = hex::encode(signature.to_bytes());

        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            signature_hex,
            None,
        );

        // Verify with correct public key
        let result = manager.verify_signature(&fed_sig, &keypair.public_key(), &metadata)?;
        assert!(result);

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_invalid_signature() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            "a".repeat(128), // 64 bytes in hex
            None,
        );

        // Verify should fail with wrong signature
        let result = manager.verify_signature(&fed_sig, &keypair.public_key(), &metadata)?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_signature_wrong_key() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair1.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        // Sign with keypair1
        let payload = serde_json::to_vec(&metadata)?;
        let signature = keypair1.sign(&payload);
        let signature_hex = hex::encode(signature.to_bytes());

        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            signature_hex,
            None,
        );

        // Verify with keypair2's public key (wrong key)
        let result = manager.verify_signature(&fed_sig, &keypair2.public_key(), &metadata)?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_signature_invalid_hex() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            "not_valid_hex!!!".to_string(),
            None,
        );

        // Should error due to invalid hex
        let result = manager.verify_signature(&fed_sig, &keypair.public_key(), &metadata);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid signature hex"));

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_signature_wrong_length() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        // Too short signature (should be 64 bytes = 128 hex chars)
        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            "deadbeef".to_string(), // Only 4 bytes
            None,
        );

        // Should error due to wrong length
        let result = manager.verify_signature(&fed_sig, &keypair.public_key(), &metadata);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid signature length"));

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_signature_tampered_metadata() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair.clone(),
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);

        // Sign original metadata
        let payload = serde_json::to_vec(&metadata)?;
        let signature = keypair.sign(&payload);
        let signature_hex = hex::encode(signature.to_bytes());

        let fed_sig = FederationSignature::new(
            "test-host".to_string(),
            metadata.merkle_root.to_string(),
            signature_hex,
            None,
        );

        // Tamper with metadata
        let tampered_metadata = create_test_metadata("different_merkle_root", None);

        // Verification should fail with tampered metadata
        let result =
            manager.verify_signature(&fed_sig, &keypair.public_key(), &tampered_metadata)?;
        assert!(!result);

        Ok(())
    }
}

// ============================================================================
// Quorum Manager Tests
// ============================================================================

mod quorum_tests {
    use super::*;

    #[tokio::test]
    async fn test_quorum_init_and_status() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        manager.init_quorum(&bundle_hash, 3).await?;

        let status = manager.get_quorum_status(&bundle_hash).await?;
        assert_eq!(status.required_signatures, 3);
        assert_eq!(status.collected_signatures, 0);
        assert!(!status.quorum_reached);

        Ok(())
    }

    #[tokio::test]
    async fn test_quorum_not_reached_with_insufficient_signatures() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let keypair = Keypair::generate();
        let message = b"test message";
        let sig = keypair.sign(message);

        manager.init_quorum(&bundle_hash, 3).await?;

        // Only one signature (need 3)
        let reached = manager
            .record_signature(&bundle_hash, "host1", &sig)
            .await?;
        assert!(!reached);

        let is_reached = manager.is_quorum_reached(&bundle_hash).await?;
        assert!(!is_reached);

        Ok(())
    }

    #[tokio::test]
    async fn test_quorum_reached_with_exact_threshold() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let message = b"test message";

        manager.init_quorum(&bundle_hash, 2).await?;

        // First signature
        let keypair1 = Keypair::generate();
        let sig1 = keypair1.sign(message);
        let reached1 = manager
            .record_signature(&bundle_hash, "host1", &sig1)
            .await?;
        assert!(!reached1);

        // Second signature - should reach quorum
        let keypair2 = Keypair::generate();
        let sig2 = keypair2.sign(message);
        let reached2 = manager
            .record_signature(&bundle_hash, "host2", &sig2)
            .await?;
        assert!(reached2);

        Ok(())
    }

    #[tokio::test]
    async fn test_quorum_build_exchange() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let message = b"test message";

        manager.init_quorum(&bundle_hash, 2).await?;

        let keypair1 = Keypair::generate();
        let sig1 = keypair1.sign(message);
        manager
            .record_signature(&bundle_hash, "host1", &sig1)
            .await?;

        let keypair2 = Keypair::generate();
        let sig2 = keypair2.sign(message);
        manager
            .record_signature(&bundle_hash, "host2", &sig2)
            .await?;

        let exchange = manager.build_exchange(&bundle_hash).await?;
        assert_eq!(exchange.signature_count(), 2);
        assert!(exchange.is_quorum_reached());

        let hosts = exchange.hosts();
        assert!(hosts.contains(&"host1".to_string()));
        assert!(hosts.contains(&"host2".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_exchange_all_valid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let message = b"test message";

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        let sig1 = keypair1.sign(message);
        let sig2 = keypair2.sign(message);

        let mut exchange = BundleSignatureExchange::new(bundle_hash, 2);
        exchange.add_signature("host1".to_string(), sig1);
        exchange.add_signature("host2".to_string(), sig2);

        let mut pubkeys = HashMap::new();
        pubkeys.insert("host1".to_string(), keypair1.public_key());
        pubkeys.insert("host2".to_string(), keypair2.public_key());

        let result = manager.verify_exchange(&exchange, &pubkeys, message)?;
        assert!(result.all_verified);
        assert!(result.quorum_verified);
        assert_eq!(result.verified_hosts.len(), 2);
        assert!(result.failed_hosts.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_exchange_one_invalid() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let message = b"test message";
        let wrong_message = b"wrong message";

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        let sig1 = keypair1.sign(message);
        let sig2 = keypair2.sign(wrong_message); // Signed wrong message

        let mut exchange = BundleSignatureExchange::new(bundle_hash, 2);
        exchange.add_signature("host1".to_string(), sig1);
        exchange.add_signature("host2".to_string(), sig2);

        let mut pubkeys = HashMap::new();
        pubkeys.insert("host1".to_string(), keypair1.public_key());
        pubkeys.insert("host2".to_string(), keypair2.public_key());

        let result = manager.verify_exchange(&exchange, &pubkeys, message)?;
        assert!(!result.all_verified);
        assert!(!result.quorum_verified); // Only 1 valid, need 2
        assert_eq!(result.verified_hosts.len(), 1);
        assert_eq!(result.failed_hosts.len(), 1);
        assert!(result.failed_hosts.contains(&"host2".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_exchange_missing_pubkey() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let message = b"test message";

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        let sig1 = keypair1.sign(message);
        let sig2 = keypair2.sign(message);

        let mut exchange = BundleSignatureExchange::new(bundle_hash, 2);
        exchange.add_signature("host1".to_string(), sig1);
        exchange.add_signature("host2".to_string(), sig2);

        // Only provide pubkey for host1
        let mut pubkeys = HashMap::new();
        pubkeys.insert("host1".to_string(), keypair1.public_key());

        let result = manager.verify_exchange(&exchange, &pubkeys, message)?;
        assert!(!result.all_verified);
        assert_eq!(result.verified_hosts.len(), 1);
        assert_eq!(result.failed_hosts.len(), 1);
        assert!(result.failed_hosts.contains(&"host2".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_quorum_status_for_nonexistent_bundle() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"nonexistent");
        let result = manager.get_quorum_status(&bundle_hash).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No quorum tracking found"));

        Ok(())
    }
}

// ============================================================================
// Bundle Signing and Storage Tests
// ============================================================================

mod bundle_signing {
    use super::*;

    #[tokio::test]
    async fn test_sign_bundle_creates_record() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db.clone(),
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);
        let sig = manager.sign_bundle(&metadata).await?;

        assert_eq!(sig.host_id, "test-host");
        assert!(!sig.signature.is_empty());
        assert_eq!(sig.bundle_hash, metadata.merkle_root.to_string());

        // Verify signature was stored
        let signatures = manager
            .get_signatures_for_bundle(&metadata.merkle_root.to_string())
            .await?;
        assert_eq!(signatures.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_sign_bundle_with_prev_hash() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_2", Some("prev_bundle".to_string()));
        let sig = manager.sign_bundle(&metadata).await?;

        assert!(sig.prev_host_hash.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_host_chain() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        // Sign multiple bundles
        for i in 0..5 {
            let metadata = create_test_metadata(&format!("merkle_root_{}", i), None);
            manager.sign_bundle(&metadata).await?;
        }

        let chain = manager.get_host_chain("test-host", 10).await?;
        assert_eq!(chain.len(), 5);

        // Check ordering (DESC by created_at)
        for i in 0..chain.len() - 1 {
            assert!(chain[i].created_at >= chain[i + 1].created_at);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_mark_signature_verified() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(
            db,
            keypair,
            "test-host".to_string(),
            "tenant-001".to_string(),
        )?;

        let metadata = create_test_metadata("merkle_root_1", None);
        let _sig = manager.sign_bundle(&metadata).await?;

        // Initially not verified
        let signatures = manager
            .get_signatures_for_bundle(&metadata.merkle_root.to_string())
            .await?;
        assert!(!signatures[0].verified);

        // Mark as verified (need to get the ID from stored signature)
        let stored = manager
            .get_signatures_for_bundle(&metadata.merkle_root.to_string())
            .await?;
        if let Some(id) = &stored[0].id {
            manager.mark_verified(id).await?;
        }

        // Check verified flag
        let signatures = manager
            .get_signatures_for_bundle(&metadata.merkle_root.to_string())
            .await?;
        assert!(signatures[0].verified);

        Ok(())
    }
}
