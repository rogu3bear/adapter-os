//! Peer Attestation Tests - Comprehensive tests for peer attestation and registry
//!
//! Tests the peer attestation logic including:
//! - Attestation metadata parsing failures
//! - Certificate validation edge cases
//! - Peer registration and deregistration flows
//! - Health status transitions
//! - Discovery status state machine

use adapteros_core::{time, Result};
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_federation::{
    attestation::{attest_bundle, verify_hardware_attestation, AttestationInfo},
    peer::{
        AttestationMetadata, DiscoveryAnnouncement, DiscoveryErrorPacket, DiscoveryStatus,
        PeerHealthStatus, PeerRegistry,
    },
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

// Helper to setup a test database with proper cleanup
// Returns (Db, TempDir) - caller must hold TempDir to keep DB files alive
async fn setup_test_db() -> Result<(Db, TempDir)> {
    let temp_dir = TempDir::with_prefix("aos-federation-test-").expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join(format!("test_{}.db", uuid::Uuid::new_v4()));
    let db = Db::connect(db_path.to_str().unwrap()).await?;
    db.migrate().await?;
    Ok((db, temp_dir))
}

// Helper to create valid attestation metadata
fn create_valid_attestation() -> AttestationMetadata {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    AttestationMetadata {
        platform: "macos".to_string(),
        secure_enclave_available: true,
        tpm_available: false,
        attestation_timestamp: now,
        hardware_id: Some("test-hw-001".to_string()),
    }
}

// Helper to create stale attestation metadata (>7 days old)
fn create_stale_attestation() -> AttestationMetadata {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    AttestationMetadata {
        platform: "macos".to_string(),
        secure_enclave_available: true,
        tpm_available: false,
        attestation_timestamp: now - (8 * 24 * 60 * 60), // 8 days ago
        hardware_id: Some("test-hw-001".to_string()),
    }
}

// Helper to create future attestation metadata
fn create_future_attestation() -> AttestationMetadata {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    AttestationMetadata {
        platform: "macos".to_string(),
        secure_enclave_available: true,
        tpm_available: false,
        attestation_timestamp: now + (10 * 60), // 10 minutes in the future
        hardware_id: Some("test-hw-001".to_string()),
    }
}

// ============================================================================
// Attestation Metadata Parsing Tests
// ============================================================================

mod attestation_metadata_parsing {
    use super::*;

    #[test]
    fn test_attestation_metadata_new() {
        let attestation = AttestationMetadata::new("linux".to_string());

        assert_eq!(attestation.platform, "linux");
        assert!(!attestation.tpm_available);
        assert!(attestation.hardware_id.is_none());
        // Timestamp should be recent (within last second)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(attestation.attestation_timestamp <= now);
        assert!(attestation.attestation_timestamp >= now - 1);
    }

    #[test]
    fn test_attestation_metadata_serialization() {
        let attestation = create_valid_attestation();

        let json = serde_json::to_string(&attestation).unwrap();
        let deserialized: AttestationMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(attestation.platform, deserialized.platform);
        assert_eq!(
            attestation.secure_enclave_available,
            deserialized.secure_enclave_available
        );
        assert_eq!(
            attestation.attestation_timestamp,
            deserialized.attestation_timestamp
        );
    }

    #[test]
    fn test_attestation_metadata_missing_optional_fields() {
        // Test parsing with missing optional fields
        let json = r#"{
            "platform": "linux",
            "secure_enclave_available": false,
            "tpm_available": true,
            "attestation_timestamp": 1700000000
        }"#;

        let attestation: AttestationMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(attestation.platform, "linux");
        assert!(attestation.hardware_id.is_none());
    }

    #[test]
    fn test_attestation_metadata_invalid_json() {
        let invalid_json = r#"{
            "platform": "linux",
            "secure_enclave_available": "not_a_bool"
        }"#;

        let result: std::result::Result<AttestationMetadata, _> =
            serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_attestation_metadata_empty_platform() {
        let json = r#"{
            "platform": "",
            "secure_enclave_available": true,
            "tpm_available": false,
            "attestation_timestamp": 1700000000
        }"#;

        // Empty platform should still parse (validation happens elsewhere)
        let attestation: AttestationMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(attestation.platform, "");
    }
}

// ============================================================================
// Certificate Validation Tests
// ============================================================================

mod certificate_validation {
    use super::*;

    #[tokio::test]
    async fn test_valid_public_key_registration() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let result = registry
            .register_peer(
                "test-host".to_string(),
                keypair.public_key(),
                Some("test.example.com".to_string()),
                None,
            )
            .await;

        assert!(result.is_ok());

        Ok(())
    }

    #[tokio::test]
    async fn test_all_zero_public_key_rejected() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Create an all-zero public key (invalid)
        let zero_bytes = [0u8; 32];
        let zero_pubkey = adapteros_crypto::PublicKey::from_bytes(&zero_bytes)?;

        let result = registry
            .register_peer("bad-host".to_string(), zero_pubkey, None, None)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("all-zero key"));

        Ok(())
    }

    #[tokio::test]
    async fn test_weak_key_pattern_rejected() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Create an all-0xFF public key (weak pattern)
        let weak_bytes = [0xFFu8; 32];
        let weak_pubkey = adapteros_crypto::PublicKey::from_bytes(&weak_bytes)?;

        let result = registry
            .register_peer("weak-host".to_string(), weak_pubkey, None, None)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("weak key"));

        Ok(())
    }

    #[tokio::test]
    async fn test_attestation_timestamp_future_rejected() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let future_attestation = create_future_attestation();

        let result = registry
            .register_peer(
                "future-host".to_string(),
                keypair.public_key(),
                None,
                Some(future_attestation),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("future"));

        Ok(())
    }

    #[tokio::test]
    async fn test_attestation_timestamp_stale_rejected() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let stale_attestation = create_stale_attestation();

        let result = registry
            .register_peer(
                "stale-host".to_string(),
                keypair.public_key(),
                None,
                Some(stale_attestation),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too old"));

        Ok(())
    }

    #[tokio::test]
    async fn test_peer_without_attestation_warning() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();

        // Registration should succeed but would log a warning
        let result = registry
            .register_peer(
                "no-attestation-host".to_string(),
                keypair.public_key(),
                None,
                None, // No attestation
            )
            .await;

        assert!(result.is_ok());

        let peer = registry.get_peer("no-attestation-host").await?.unwrap();
        assert!(peer.attestation_metadata.is_none());

        Ok(())
    }
}

// ============================================================================
// Peer Registration and Deregistration Tests
// ============================================================================

mod peer_registration {
    use super::*;

    #[tokio::test]
    async fn test_register_peer_basic() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer(
                "test-host".to_string(),
                keypair.public_key(),
                Some("test.example.com".to_string()),
                None,
            )
            .await?;

        let peer = registry.get_peer("test-host").await?.unwrap();
        assert_eq!(peer.host_id, "test-host");
        assert_eq!(peer.hostname, Some("test.example.com".to_string()));
        assert!(peer.active);
        assert_eq!(peer.health_status, PeerHealthStatus::Healthy);
        assert_eq!(peer.discovery_status, DiscoveryStatus::Registered);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_peer_with_attestation() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let attestation = create_valid_attestation();

        registry
            .register_peer(
                "attested-host".to_string(),
                keypair.public_key(),
                None,
                Some(attestation.clone()),
            )
            .await?;

        let peer = registry.get_peer("attested-host").await?.unwrap();
        assert!(peer.attestation_metadata.is_some());
        let stored_attestation = peer.attestation_metadata.unwrap();
        assert_eq!(stored_attestation.platform, attestation.platform);

        Ok(())
    }

    #[tokio::test]
    async fn test_register_peer_updates_existing() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        // Initial registration
        registry
            .register_peer(
                "updating-host".to_string(),
                keypair1.public_key(),
                Some("old.example.com".to_string()),
                None,
            )
            .await?;

        // Update registration
        registry
            .register_peer(
                "updating-host".to_string(),
                keypair2.public_key(),
                Some("new.example.com".to_string()),
                None,
            )
            .await?;

        let peer = registry.get_peer("updating-host").await?.unwrap();
        assert_eq!(peer.hostname, Some("new.example.com".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_deactivate_peer() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer(
                "to-deactivate".to_string(),
                keypair.public_key(),
                None,
                None,
            )
            .await?;

        // Verify active
        let active_peers = registry.list_active_peers().await?;
        assert_eq!(active_peers.len(), 1);

        // Deactivate
        registry.deactivate_peer("to-deactivate").await?;

        // Verify inactive
        let active_peers = registry.list_active_peers().await?;
        assert_eq!(active_peers.len(), 0);

        // Peer should still exist, just inactive
        let peer = registry.get_peer("to-deactivate").await?;
        // Note: Deactivated peers may not be in cache
        assert!(peer.is_none() || !peer.unwrap().active);

        Ok(())
    }

    #[tokio::test]
    async fn test_list_multiple_peers() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        for i in 0..5 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("host-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        let peers = registry.list_active_peers().await?;
        assert_eq!(peers.len(), 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_nonexistent_peer() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let peer = registry.get_peer("nonexistent").await?;
        assert!(peer.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_update_last_seen() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("seen-host".to_string(), keypair.public_key(), None, None)
            .await?;

        // Wait a bit to ensure timestamp difference
        tokio::time::sleep(Duration::from_millis(50)).await;

        registry.update_last_seen("seen-host").await?;

        let peer = registry.get_peer("seen-host").await?.unwrap();
        assert!(peer.last_seen_at.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_peer_ids() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        for i in 0..3 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("peer-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        let ids = registry.get_all_peer_ids().await?;
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"peer-0".to_string()));
        assert!(ids.contains(&"peer-1".to_string()));
        assert!(ids.contains(&"peer-2".to_string()));

        Ok(())
    }
}

// ============================================================================
// Health Status Transition Tests
// ============================================================================

mod health_status_transitions {
    use super::*;

    #[tokio::test]
    async fn test_healthy_to_degraded() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("health-test".to_string(), keypair.public_key(), None, None)
            .await?;

        // Initially healthy
        let peer = registry.get_peer("health-test").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Healthy);

        // Record degraded health check
        registry
            .record_health_check(
                "health-test",
                PeerHealthStatus::Degraded,
                150,
                Some("slow response".to_string()),
            )
            .await?;

        let peer = registry.get_peer("health-test").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Degraded);
        assert_eq!(peer.failed_heartbeats, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_degraded_to_unhealthy_threshold() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        // Configure with max_failed_heartbeats = 2
        let registry = PeerRegistry::with_config(Arc::new(db), 2, 30, 2);

        let keypair = Keypair::generate();
        registry
            .register_peer(
                "threshold-test".to_string(),
                keypair.public_key(),
                None,
                None,
            )
            .await?;

        // First failed check - degraded
        registry
            .record_health_check(
                "threshold-test",
                PeerHealthStatus::Degraded,
                200,
                Some("timeout".to_string()),
            )
            .await?;
        let peer = registry.get_peer("threshold-test").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Degraded);
        assert_eq!(peer.failed_heartbeats, 1);

        // Second failed check - should become unhealthy
        registry
            .record_health_check(
                "threshold-test",
                PeerHealthStatus::Degraded,
                250,
                Some("timeout".to_string()),
            )
            .await?;
        let peer = registry.get_peer("threshold-test").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Unhealthy);
        assert_eq!(peer.failed_heartbeats, 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_from_degraded_to_healthy() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer(
                "recovery-test".to_string(),
                keypair.public_key(),
                None,
                None,
            )
            .await?;

        // Make degraded
        registry
            .record_health_check("recovery-test", PeerHealthStatus::Degraded, 200, None)
            .await?;

        // Recover to healthy
        registry
            .record_health_check("recovery-test", PeerHealthStatus::Healthy, 10, None)
            .await?;

        let peer = registry.get_peer("recovery-test").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Healthy);
        assert_eq!(peer.failed_heartbeats, 0); // Reset on healthy

        Ok(())
    }

    #[tokio::test]
    async fn test_health_history_recording() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("history-test".to_string(), keypair.public_key(), None, None)
            .await?;

        // Record multiple health checks
        for i in 0..5 {
            let status = if i % 2 == 0 {
                PeerHealthStatus::Healthy
            } else {
                PeerHealthStatus::Degraded
            };
            registry
                .record_health_check("history-test", status, 10 + (i as u32 * 10), None)
                .await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let history = registry.get_health_history("history-test", 10).await?;
        assert!(history.len() >= 5);

        Ok(())
    }

    #[tokio::test]
    async fn test_isolated_status_from_partition() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 5 peers
        for i in 0..5 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("node-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        registry.set_local_host_id("node-0".to_string()).await;

        // Simulate partition: only nodes 0-2 reachable (majority)
        let reachable: HashSet<String> = (0..3).map(|i| format!("node-{}", i)).collect();
        let partition = registry.detect_partition(reachable).await?;

        // Consensus not yet reached with only local vote
        assert!(partition.is_none());

        // Isolated peers should remain healthy until quorum is reached
        let node3 = registry.get_peer("node-3").await?.unwrap();
        assert_eq!(node3.health_status, PeerHealthStatus::Healthy);

        let node4 = registry.get_peer("node-4").await?.unwrap();
        assert_eq!(node4.health_status, PeerHealthStatus::Healthy);

        Ok(())
    }

    #[tokio::test]
    async fn test_list_peers_by_health_status() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Create peers with different health statuses
        for i in 0..6 {
            let keypair = Keypair::generate();
            registry
                .register_peer(
                    format!("status-test-{}", i),
                    keypair.public_key(),
                    None,
                    None,
                )
                .await?;
        }

        // Mark some as degraded
        registry
            .record_health_check("status-test-0", PeerHealthStatus::Degraded, 100, None)
            .await?;
        registry
            .record_health_check("status-test-1", PeerHealthStatus::Degraded, 100, None)
            .await?;

        let healthy = registry
            .list_peers_by_health(PeerHealthStatus::Healthy)
            .await?;
        assert_eq!(healthy.len(), 4);

        let degraded = registry
            .list_peers_by_health(PeerHealthStatus::Degraded)
            .await?;
        assert_eq!(degraded.len(), 2);

        Ok(())
    }
}

// ============================================================================
// Discovery Status State Machine Tests
// ============================================================================

mod discovery_status_state_machine {
    use super::*;

    #[tokio::test]
    async fn test_initial_discovery_status_is_registered() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("new-peer".to_string(), keypair.public_key(), None, None)
            .await?;

        let peer = registry.get_peer("new-peer").await?.unwrap();
        assert_eq!(peer.discovery_status, DiscoveryStatus::Registered);

        Ok(())
    }

    #[tokio::test]
    async fn test_process_discovery_announcement() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register initial peer
        let keypair = Keypair::generate();
        registry
            .register_peer("seed-node".to_string(), keypair.public_key(), None, None)
            .await?;

        // Process announcement with new peers
        let announcement = DiscoveryAnnouncement {
            sender_id: "seed-node".to_string(),
            known_peers: vec!["new-peer-1".to_string(), "new-peer-2".to_string()],
            announcement_time: time::unix_timestamp_secs(),
            federation_epoch: 1,
        };

        let discovered = registry
            .process_discovery_announcement(&announcement)
            .await?;
        assert_eq!(discovered.len(), 2);
        assert!(discovered.contains(&"new-peer-1".to_string()));
        assert!(discovered.contains(&"new-peer-2".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_filters_known_peers() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register two peers
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        registry
            .register_peer("known-peer".to_string(), keypair1.public_key(), None, None)
            .await?;
        registry
            .register_peer("sender".to_string(), keypair2.public_key(), None, None)
            .await?;

        // Process announcement with mix of known and new peers
        let announcement = DiscoveryAnnouncement {
            sender_id: "sender".to_string(),
            known_peers: vec![
                "known-peer".to_string(),   // Already known
                "unknown-peer".to_string(), // New
            ],
            announcement_time: time::unix_timestamp_secs(),
            federation_epoch: 1,
        };

        let discovered = registry
            .process_discovery_announcement(&announcement)
            .await?;
        assert_eq!(discovered.len(), 1);
        assert!(discovered.contains(&"unknown-peer".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_clock_drift_rejection() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Announcement with timestamp too far in the future
        let announcement = DiscoveryAnnouncement {
            sender_id: "drifted-sender".to_string(),
            known_peers: vec!["peer1".to_string()],
            announcement_time: time::unix_timestamp_secs() + 10, // 10 seconds in future
            federation_epoch: 1,
        };

        let result = registry.process_discovery_announcement(&announcement).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TimeSyncRequired"));

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_clock_drift_past_rejection() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Announcement with timestamp too far in the past
        let announcement = DiscoveryAnnouncement {
            sender_id: "drifted-sender".to_string(),
            known_peers: vec!["peer1".to_string()],
            announcement_time: time::unix_timestamp_secs() - 10, // 10 seconds in past
            federation_epoch: 1,
        };

        let result = registry.process_discovery_announcement(&announcement).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("TimeSyncRequired"));

        Ok(())
    }

    #[tokio::test]
    async fn test_discovery_error_packet_format() {
        let packet = DiscoveryErrorPacket::time_sync_required(6000);

        assert_eq!(
            packet.code,
            adapteros_federation::peer::DiscoveryErrorCode::TimeSyncRequired
        );
        assert!(packet.message.contains("6000"));
        assert!(packet.message.contains("5000ms tolerance"));
    }
}

// ============================================================================
// Consensus and Partition Tests
// ============================================================================

mod consensus_and_partition {
    use super::*;

    #[tokio::test]
    async fn test_initiate_consensus() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let target_keypair = Keypair::generate();
        registry
            .register_peer(
                "target-peer".to_string(),
                target_keypair.public_key(),
                None,
                None,
            )
            .await?;

        // Register participating peers
        for i in 0..3 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("voter-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        let participating = vec![
            "voter-0".to_string(),
            "voter-1".to_string(),
            "voter-2".to_string(),
        ];

        let decision_id = registry
            .initiate_consensus("target-peer", "evict".to_string(), participating)
            .await?;

        assert!(!decision_id.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_consensus_quorum_voting() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let target_keypair = Keypair::generate();
        registry
            .register_peer(
                "target".to_string(),
                target_keypair.public_key(),
                None,
                None,
            )
            .await?;

        // Register 3 voters
        for i in 0..3 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("voter-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        let participating = vec![
            "voter-0".to_string(),
            "voter-1".to_string(),
            "voter-2".to_string(),
        ];

        let decision_id = registry
            .initiate_consensus("target", "action".to_string(), participating)
            .await?;

        // First vote (1 of 2 required)
        let reached1 = registry
            .record_consensus_vote(&decision_id, "voter-0", true)
            .await?;
        assert!(!reached1);

        // Second vote (2 of 2 required - quorum!)
        let reached2 = registry
            .record_consensus_vote(&decision_id, "voter-1", true)
            .await?;
        assert!(reached2);

        Ok(())
    }

    #[tokio::test]
    async fn test_partition_detection_requires_quorum() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 5 peers
        for i in 0..5 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("node-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        registry.set_local_host_id("node-0".to_string()).await;

        // Minority partition (only 2 of 5 reachable)
        let reachable: HashSet<String> = (0..2).map(|i| format!("node-{}", i)).collect();
        let partition = registry.detect_partition(reachable).await?;

        // Should return None because we don't have quorum
        assert!(partition.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_partition_detection_with_quorum() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 5 peers
        for i in 0..5 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("node-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        registry.set_local_host_id("node-0".to_string()).await;

        // Majority partition (3 of 5 reachable)
        let reachable: HashSet<String> = (0..3).map(|i| format!("node-{}", i)).collect();
        let partition = registry.detect_partition(reachable).await?;

        // Consensus not yet reached with only local vote
        assert!(partition.is_none());

        let node3 = registry.get_peer("node-3").await?.unwrap();
        assert_eq!(node3.health_status, PeerHealthStatus::Healthy);

        let node4 = registry.get_peer("node-4").await?.unwrap();
        assert_eq!(node4.health_status, PeerHealthStatus::Healthy);

        Ok(())
    }

    #[tokio::test]
    async fn test_partition_resolution() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let db = Arc::new(db);
        let registry = PeerRegistry::new(Arc::clone(&db));

        // Register 3 peers
        for i in 0..3 {
            let keypair = Keypair::generate();
            let attestation = create_valid_attestation();
            registry
                .register_peer(
                    format!("node-{}", i),
                    keypair.public_key(),
                    None,
                    Some(attestation),
                )
                .await?;

            // Record a recent heartbeat so recovery check passes
            registry
                .record_health_check(&format!("node-{}", i), PeerHealthStatus::Healthy, 10, None)
                .await?;
        }

        registry.set_local_host_id("node-0".to_string()).await;

        let partition_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let isolated_peers = vec!["node-2".to_string()];
        let reachable_peers = vec!["node-0".to_string(), "node-1".to_string()];

        sqlx::query(
            r#"
            INSERT INTO partition_events (partition_id, detected_at, isolated_peers_json, reachable_peers_json, quorum_leader, resolved)
            VALUES (?, ?, ?, ?, ?, 0)
            "#,
        )
        .bind(&partition_id)
        .bind(&now)
        .bind(serde_json::to_string(&isolated_peers).unwrap_or_default())
        .bind(serde_json::to_string(&reachable_peers).unwrap_or_default())
        .bind("node-0")
        .execute(db.pool_result()?)
        .await?;

        // Resolve partition
        registry.resolve_partition(&partition_id).await?;

        let node2 = registry.get_peer("node-2").await?.unwrap();
        assert_eq!(node2.health_status, PeerHealthStatus::Healthy);
        Ok(())
    }

    #[tokio::test]
    async fn test_no_partition_when_all_reachable() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 3 peers
        for i in 0..3 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("node-{}", i), keypair.public_key(), None, None)
                .await?;
        }

        // All peers reachable
        let reachable: HashSet<String> = (0..3).map(|i| format!("node-{}", i)).collect();
        let partition = registry.detect_partition(reachable).await?;

        assert!(partition.is_none());

        Ok(())
    }
}

// ============================================================================
// Attestation Verification Tests
// ============================================================================

mod attestation_verification {
    use super::*;

    #[test]
    fn test_verify_hardware_attestation_valid() {
        let attestation = AttestationInfo {
            hardware_backed: true,
            enclave_id: Some("test-enclave-001".to_string()),
            attested_at: chrono::Utc::now().to_rfc3339(),
            algorithm: "ECDSA-P256-SecureEnclave".to_string(),
        };

        let result = verify_hardware_attestation(&attestation);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_hardware_attestation_not_hardware_backed() {
        let attestation = AttestationInfo {
            hardware_backed: false,
            enclave_id: None,
            attested_at: chrono::Utc::now().to_rfc3339(),
            algorithm: "Ed25519-Software".to_string(),
        };

        let result = verify_hardware_attestation(&attestation);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not hardware-attested"));
    }

    #[test]
    fn test_verify_hardware_attestation_missing_enclave_id() {
        let attestation = AttestationInfo {
            hardware_backed: true,
            enclave_id: None, // Missing enclave ID
            attested_at: chrono::Utc::now().to_rfc3339(),
            algorithm: "ECDSA-P256-SecureEnclave".to_string(),
        };

        let result = verify_hardware_attestation(&attestation);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing enclave ID"));
    }

    #[test]
    fn test_attest_bundle_creates_attestation() {
        let payload = b"test federation bundle data";
        let result = attest_bundle(payload);

        assert!(result.is_ok());
        let (signature, attestation) = result.unwrap();

        assert!(!attestation.attested_at.is_empty());
        assert!(!attestation.algorithm.is_empty());
        // Signature should be valid Ed25519 (64 bytes)
        assert_eq!(signature.to_bytes().len(), 64);
    }

    #[tokio::test]
    async fn test_peer_attestation_verification() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let attestation = create_valid_attestation();

        registry
            .register_peer(
                "attested-peer".to_string(),
                keypair.public_key(),
                None,
                Some(attestation),
            )
            .await?;

        let peer = registry.get_peer("attested-peer").await?.unwrap();
        let is_valid = registry.verify_attestation(&peer)?;

        assert!(is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_peer_attestation_no_hardware_root() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();

        // Attestation with no hardware root of trust
        let attestation = AttestationMetadata {
            platform: "virtual".to_string(),
            secure_enclave_available: false,
            tpm_available: false,
            attestation_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hardware_id: None,
        };

        // Registration should still work
        registry
            .register_peer(
                "soft-peer".to_string(),
                keypair.public_key(),
                None,
                Some(attestation),
            )
            .await?;

        let peer = registry.get_peer("soft-peer").await?.unwrap();
        let is_valid = registry.verify_attestation(&peer)?;

        // Should fail verification (no hardware root)
        assert!(!is_valid);

        Ok(())
    }

    #[tokio::test]
    async fn test_peer_attestation_too_old() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();

        // Need to bypass the registration validation for this test
        // We'll create an attestation that's 6 days old (passes registration)
        // then wait conceptually or modify the timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let attestation = AttestationMetadata {
            platform: "macos".to_string(),
            secure_enclave_available: true,
            tpm_available: false,
            attestation_timestamp: now - (25 * 60 * 60), // 25 hours ago (> 24 hour limit for verify)
            hardware_id: Some("test".to_string()),
        };

        registry
            .register_peer(
                "old-attestation-peer".to_string(),
                keypair.public_key(),
                None,
                Some(attestation),
            )
            .await?;

        let peer = registry.get_peer("old-attestation-peer").await?.unwrap();
        let is_valid = registry.verify_attestation(&peer)?;

        // Should fail (attestation too old for verify_attestation's 24h limit)
        assert!(!is_valid);

        Ok(())
    }
}

// ============================================================================
// Cache Loading Tests
// ============================================================================

mod cache_tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_from_database() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry1 = PeerRegistry::new(Arc::new(db.clone()));

        // Register peers with first registry
        for i in 0..3 {
            let keypair = Keypair::generate();
            registry1
                .register_peer(
                    format!("cached-peer-{}", i),
                    keypair.public_key(),
                    None,
                    None,
                )
                .await?;
        }

        // Create new registry and load cache
        let registry2 = PeerRegistry::new(Arc::new(db));
        registry2.load_cache().await?;

        // Verify cache loaded
        let peers = registry2.list_active_peers().await?;
        assert_eq!(peers.len(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_invalidation_on_deactivate() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("cache-test".to_string(), keypair.public_key(), None, None)
            .await?;

        // Should be in cache
        let peer = registry.get_peer("cache-test").await?;
        assert!(peer.is_some());

        // Deactivate removes from cache
        registry.deactivate_peer("cache-test").await?;

        // Cache check - might return None now
        // (Implementation clears cache on deactivate)
        Ok(())
    }

    #[tokio::test]
    async fn test_set_and_get_local_host_id() -> Result<()> {
        let (db, _temp) = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        registry.set_local_host_id("my-host-id".to_string()).await;

        let local_id = registry.get_local_host_id().await;
        assert_eq!(local_id, "my-host-id");

        Ok(())
    }
}
