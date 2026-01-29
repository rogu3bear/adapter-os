//! Federation Peer Discovery Integration Tests
//! Multi-peer federation scenarios including peer discovery, health checking, consensus, etc.

use adapteros_core::time;
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_federation::peer::{
    AttestationMetadata, DiscoveryAnnouncement, PeerHealthStatus, PeerRegistry,
};
use std::collections::HashSet;
use std::sync::Arc;

async fn setup_registry() -> adapteros_core::Result<(Arc<Db>, PeerRegistry)> {
    let db = Arc::new(Db::new_in_memory().await?);
    let registry = PeerRegistry::new(db.clone());
    Ok((db, registry))
}

async fn register_peer(registry: &PeerRegistry, host_id: &str) -> adapteros_core::Result<()> {
    let keypair = Keypair::generate();
    let pubkey = keypair.public_key();
    let attestation = AttestationMetadata::new("test".to_string());
    registry
        .register_peer(
            host_id.to_string(),
            pubkey,
            Some(format!("{}.local", host_id)),
            Some(attestation),
        )
        .await
}

#[tokio::test]
async fn test_peer_discovery() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    let announcement = DiscoveryAnnouncement {
        sender_id: "peer-1".to_string(),
        known_peers: vec!["peer-2".to_string()],
        announcement_time: time::unix_timestamp_secs(),
        federation_epoch: 1,
    };

    let discovered = registry
        .process_discovery_announcement(&announcement)
        .await?;
    assert_eq!(discovered, vec!["peer-2".to_string()]);
    Ok(())
}

#[tokio::test]
async fn test_peer_health_check() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    registry
        .record_health_check("peer-1", PeerHealthStatus::Degraded, 120, None)
        .await?;

    let degraded = registry
        .list_peers_by_health(PeerHealthStatus::Degraded)
        .await?;
    assert_eq!(degraded.len(), 1);
    assert_eq!(degraded[0].host_id, "peer-1");
    Ok(())
}

#[tokio::test]
async fn test_consensus_voting() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;
    register_peer(&registry, "peer-2").await?;
    register_peer(&registry, "peer-3").await?;

    let decision_id = registry
        .initiate_consensus(
            "peer-2",
            "promote".to_string(),
            vec![
                "peer-1".to_string(),
                "peer-2".to_string(),
                "peer-3".to_string(),
            ],
        )
        .await?;

    let quorum1 = registry
        .record_consensus_vote(&decision_id, "peer-1", true)
        .await?;
    assert!(!quorum1);

    let quorum2 = registry
        .record_consensus_vote(&decision_id, "peer-2", true)
        .await?;
    assert!(quorum2);
    Ok(())
}

#[tokio::test]
async fn test_network_partition() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;
    register_peer(&registry, "peer-2").await?;
    register_peer(&registry, "peer-3").await?;
    registry.set_local_host_id("peer-1".to_string()).await;

    let reachable: HashSet<String> = ["peer-1".to_string(), "peer-2".to_string()]
        .into_iter()
        .collect();
    let partition = registry.detect_partition(reachable).await?;
    assert!(
        partition.is_none(),
        "partition should wait for quorum votes"
    );
    Ok(())
}

#[tokio::test]
async fn test_multi_peer_coordination() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;
    register_peer(&registry, "peer-2").await?;

    let peers = registry.list_active_peers().await?;
    assert_eq!(peers.len(), 2);
    Ok(())
}

#[tokio::test]
async fn test_peer_registration() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    let peer = registry.get_peer("peer-1").await?;
    assert!(peer.is_some());
    Ok(())
}

#[tokio::test]
async fn test_peer_deregistration() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;
    registry.deactivate_peer("peer-1").await?;

    let peers = registry.list_active_peers().await?;
    assert!(peers.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_peer_reconnection() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;
    registry.deactivate_peer("peer-1").await?;
    register_peer(&registry, "peer-1").await?;

    let peers = registry.list_active_peers().await?;
    assert_eq!(peers.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_peer_timeout() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    registry
        .record_health_check("peer-1", PeerHealthStatus::Unhealthy, 0, None)
        .await?;
    registry
        .record_health_check("peer-1", PeerHealthStatus::Unhealthy, 0, None)
        .await?;
    registry
        .record_health_check("peer-1", PeerHealthStatus::Unhealthy, 0, None)
        .await?;

    let unhealthy = registry
        .list_peers_by_health(PeerHealthStatus::Unhealthy)
        .await?;
    assert_eq!(unhealthy.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_peer_quarantine() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    registry
        .record_health_check(
            "peer-1",
            PeerHealthStatus::Isolated,
            0,
            Some("quarantine".to_string()),
        )
        .await?;

    let isolated = registry
        .list_peers_by_health(PeerHealthStatus::Isolated)
        .await?;
    assert_eq!(isolated.len(), 1);
    Ok(())
}

#[tokio::test]
async fn test_federation_state_sync() -> adapteros_core::Result<()> {
    let (_db, registry) = setup_registry().await?;
    register_peer(&registry, "peer-1").await?;

    registry.load_cache().await?;
    let peer = registry.get_peer("peer-1").await?;
    assert!(peer.is_some());
    Ok(())
}
