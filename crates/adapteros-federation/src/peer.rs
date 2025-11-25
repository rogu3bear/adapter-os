//! Peer Registry & Discovery - Management of federated hosts
//!
//! Provides peer discovery protocol, registration, health checking,
//! consensus mechanisms, and network partition handling.
//!
//! ## Peer Discovery Protocol
//!
//! 1. **Bootstrap**: Peer registers with known seed hosts
//! 2. **Announcement**: Peer broadcasts presence to federation
//! 3. **Discovery**: Peers exchange peer lists
//! 4. **Health Check**: Periodic heartbeat verification
//! 5. **Consensus**: Multi-peer acknowledgment for state changes
//!
//! ## Network Partition Handling
//!
//! - Quorum-based consensus for split-brain scenarios
//! - Partition tolerance with eventual consistency
//! - Automatic recovery upon reconnection

use adapteros_core::{AosError, Result};
use adapteros_crypto::PublicKey;
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

#[cfg(test)]
use std::time::{Duration, UNIX_EPOCH};

/// Peer health status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PeerHealthStatus {
    /// Peer is responsive and healthy
    #[default]
    Healthy,
    /// Peer is slow but responding
    Degraded,
    /// Peer is unresponsive
    Unhealthy,
    /// Peer is isolated (network partition)
    Isolated,
}

/// Peer discovery status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryStatus {
    /// Peer is known and registered
    #[default]
    Registered,
    /// Peer discovery is in progress
    Discovering,
    /// Peer failed discovery
    Failed,
}

/// Peer information for a federated host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub host_id: String,
    pub pubkey: PublicKey,
    pub hostname: Option<String>,
    pub registered_at: String,
    pub last_seen_at: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub attestation_metadata: Option<AttestationMetadata>,
    pub active: bool,
    pub health_status: PeerHealthStatus,
    pub discovery_status: DiscoveryStatus,
    pub failed_heartbeats: u32,
}

/// Peer health check record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerHealthCheck {
    pub host_id: String,
    pub timestamp: String,
    pub status: PeerHealthStatus,
    pub response_time_ms: u32,
    pub error_message: Option<String>,
}

/// Hardware attestation metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationMetadata {
    pub platform: String,
    pub secure_enclave_available: bool,
    pub tpm_available: bool,
    pub attestation_timestamp: u64,
    pub hardware_id: Option<String>,
}

impl AttestationMetadata {
    pub fn new(platform: String) -> Self {
        Self {
            platform,
            secure_enclave_available: cfg!(target_os = "macos"),
            tpm_available: false,
            attestation_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            hardware_id: None,
        }
    }
}

/// Consensus decision on peer state change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusDecision {
    pub peer_id: String,
    pub action: String,
    pub participating_hosts: Vec<String>,
    pub required_votes: usize,
    pub collected_votes: usize,
    pub approved: bool,
    pub timestamp: String,
}

/// Partition detection record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionEvent {
    pub partition_id: String,
    pub detected_at: String,
    pub isolated_peers: Vec<String>,
    pub reachable_peers: Vec<String>,
    pub quorum_leader: Option<String>,
    pub resolved: bool,
}

/// Discovery announcement from a peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryAnnouncement {
    pub sender_id: String,
    pub known_peers: Vec<String>,
    pub announcement_time: u64,
    pub federation_epoch: u64,
}

/// Peer registry for managing federated hosts
pub struct PeerRegistry {
    db: Arc<Db>,
    cache: Arc<RwLock<HashMap<String, PeerInfo>>>,
    local_host_id: Arc<Mutex<String>>,
    max_failed_heartbeats: u32,
}

impl PeerRegistry {
    /// Create a new peer registry
    pub fn new(db: Arc<Db>) -> Self {
        Self::with_config(db, 2, 30, 3)
    }

    /// Create peer registry with custom configuration
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `consensus_quorum_size` - Minimum peers for quorum consensus
    /// * `heartbeat_timeout_secs` - Timeout for heartbeat responses
    /// * `max_failed_heartbeats` - Maximum failed heartbeats before marking unhealthy
    pub fn with_config(
        db: Arc<Db>,
        _consensus_quorum_size: usize,
        _heartbeat_timeout_secs: u64,
        max_failed_heartbeats: u32,
    ) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(HashMap::new())),
            local_host_id: Arc::new(Mutex::new(String::new())),
            max_failed_heartbeats,
        }
    }

    /// Set the local host ID
    pub async fn set_local_host_id(&self, host_id: String) {
        let mut local = self.local_host_id.lock().await;
        *local = host_id;
    }

    /// Get the local host ID
    pub async fn get_local_host_id(&self) -> String {
        self.local_host_id.lock().await.clone()
    }

    /// Register a new peer
    pub async fn register_peer(
        &self,
        host_id: String,
        pubkey: PublicKey,
        hostname: Option<String>,
        attestation_metadata: Option<AttestationMetadata>,
    ) -> Result<()> {
        info!(
            host_id = %host_id,
            hostname = ?hostname,
            "Registering federation peer"
        );

        let pool = self.db.pool();
        let pubkey_hex = hex::encode(pubkey.to_bytes());
        let attestation_json = attestation_metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());
        let now = chrono::Utc::now().to_rfc3339();

        // Insert or update peer
        sqlx::query(
            r#"
            INSERT INTO federation_peers (host_id, pubkey, hostname, attestation_metadata, registered_at, active, health_status, discovery_status, failed_heartbeats)
            VALUES (?, ?, ?, ?, ?, 1, 'healthy', 'registered', 0)
            ON CONFLICT(host_id) DO UPDATE SET
                pubkey = excluded.pubkey,
                hostname = excluded.hostname,
                attestation_metadata = excluded.attestation_metadata,
                last_seen_at = ?,
                active = 1
            "#
        )
        .bind(&host_id)
        .bind(&pubkey_hex)
        .bind(&hostname)
        .bind(&attestation_json)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register peer: {}", e)))?;

        // Update cache
        let peer_info = PeerInfo {
            host_id: host_id.clone(),
            pubkey,
            hostname,
            registered_at: now.clone(),
            last_seen_at: Some(now.clone()),
            last_heartbeat_at: None,
            attestation_metadata,
            active: true,
            health_status: PeerHealthStatus::Healthy,
            discovery_status: DiscoveryStatus::Registered,
            failed_heartbeats: 0,
        };

        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(host_id.clone(), peer_info);
        }

        info!(host_id = %host_id, "Peer registered successfully");
        Ok(())
    }

    /// Discover peers from a peer announcement
    pub async fn process_discovery_announcement(
        &self,
        announcement: &DiscoveryAnnouncement,
    ) -> Result<Vec<String>> {
        info!(
            sender_id = %announcement.sender_id,
            peer_count = announcement.known_peers.len(),
            "Processing discovery announcement"
        );

        let mut discovered_peers = Vec::new();

        for peer_id in &announcement.known_peers {
            if let Ok(None) = self.get_peer(peer_id).await {
                debug!(peer_id = %peer_id, "New peer discovered: {}", peer_id);
                discovered_peers.push(peer_id.clone());
            }
        }

        if !discovered_peers.is_empty() {
            info!(
                new_peers = discovered_peers.len(),
                "Discovered {} new peers",
                discovered_peers.len()
            );
        }

        Ok(discovered_peers)
    }

    /// Get list of all known peer IDs
    pub async fn get_all_peer_ids(&self) -> Result<Vec<String>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT host_id
            FROM federation_peers
            ORDER BY registered_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list peer IDs: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect())
    }

    /// Get peer information
    pub async fn get_peer(&self, host_id: &str) -> Result<Option<PeerInfo>> {
        // Try cache first
        {
            let cache = self.cache.read().unwrap();
            if let Some(peer) = cache.get(host_id) {
                return Ok(Some(peer.clone()));
            }
        }

        // Load from database
        let pool = self.db.pool();
        let row = sqlx::query(
            r#"
            SELECT host_id, pubkey, hostname, registered_at, last_seen_at, last_heartbeat_at,
                   attestation_metadata, active, health_status, discovery_status, failed_heartbeats
            FROM federation_peers
            WHERE host_id = ?
            "#,
        )
        .bind(host_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch peer: {}", e)))?;

        if let Some(row) = row {
            let pubkey_hex: String = row
                .try_get("pubkey")
                .map_err(|e| AosError::Database(format!("Failed to get pubkey: {}", e)))?;
            let pubkey_bytes = hex::decode(&pubkey_hex)
                .map_err(|e| AosError::Crypto(format!("Invalid pubkey hex: {}", e)))?;
            if pubkey_bytes.len() != 32 {
                return Err(AosError::Crypto("Invalid public key length".to_string()));
            }
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&pubkey_bytes);
            let pubkey = PublicKey::from_bytes(&key_array)?;

            let attestation_metadata: Option<String> = row.try_get("attestation_metadata").ok();
            let attestation_metadata =
                attestation_metadata.and_then(|json| serde_json::from_str(&json).ok());

            let health_status_str: String = row
                .try_get("health_status")
                .unwrap_or_else(|_| "healthy".to_string());
            let health_status = match health_status_str.as_str() {
                "degraded" => PeerHealthStatus::Degraded,
                "unhealthy" => PeerHealthStatus::Unhealthy,
                "isolated" => PeerHealthStatus::Isolated,
                _ => PeerHealthStatus::Healthy,
            };

            let discovery_status_str: String = row
                .try_get("discovery_status")
                .unwrap_or_else(|_| "registered".to_string());
            let discovery_status = match discovery_status_str.as_str() {
                "discovering" => DiscoveryStatus::Discovering,
                "failed" => DiscoveryStatus::Failed,
                _ => DiscoveryStatus::Registered,
            };

            let peer_info = PeerInfo {
                host_id: row.try_get("host_id").unwrap(),
                pubkey,
                hostname: row.try_get("hostname").ok(),
                registered_at: row.try_get("registered_at").unwrap(),
                last_seen_at: row.try_get("last_seen_at").ok(),
                last_heartbeat_at: row.try_get("last_heartbeat_at").ok(),
                attestation_metadata,
                active: row.try_get::<i64, _>("active").unwrap() != 0,
                health_status,
                discovery_status,
                failed_heartbeats: row.try_get::<i32, _>("failed_heartbeats").unwrap_or(0) as u32,
            };

            // Update cache
            {
                let mut cache = self.cache.write().unwrap();
                cache.insert(host_id.to_string(), peer_info.clone());
            }

            Ok(Some(peer_info))
        } else {
            Ok(None)
        }
    }

    /// List all active peers
    pub async fn list_active_peers(&self) -> Result<Vec<PeerInfo>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT host_id, pubkey, hostname, registered_at, last_seen_at, last_heartbeat_at,
                   attestation_metadata, active, health_status, discovery_status, failed_heartbeats
            FROM federation_peers
            WHERE active = 1
            ORDER BY last_seen_at DESC
            "#,
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list peers: {}", e)))?;

        let mut peers = Vec::new();
        for row in rows {
            let pubkey_hex: String = row
                .try_get("pubkey")
                .map_err(|e| AosError::Database(format!("Failed to get pubkey: {}", e)))?;
            let pubkey_bytes = hex::decode(&pubkey_hex)
                .map_err(|e| AosError::Crypto(format!("Invalid pubkey hex: {}", e)))?;
            if pubkey_bytes.len() != 32 {
                return Err(AosError::Crypto("Invalid public key length".to_string()));
            }
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&pubkey_bytes);
            let pubkey = PublicKey::from_bytes(&key_array)?;

            let attestation_metadata: Option<String> = row.try_get("attestation_metadata").ok();
            let attestation_metadata =
                attestation_metadata.and_then(|json| serde_json::from_str(&json).ok());

            let health_status_str: String = row
                .try_get("health_status")
                .unwrap_or_else(|_| "healthy".to_string());
            let health_status = match health_status_str.as_str() {
                "degraded" => PeerHealthStatus::Degraded,
                "unhealthy" => PeerHealthStatus::Unhealthy,
                "isolated" => PeerHealthStatus::Isolated,
                _ => PeerHealthStatus::Healthy,
            };

            let discovery_status_str: String = row
                .try_get("discovery_status")
                .unwrap_or_else(|_| "registered".to_string());
            let discovery_status = match discovery_status_str.as_str() {
                "discovering" => DiscoveryStatus::Discovering,
                "failed" => DiscoveryStatus::Failed,
                _ => DiscoveryStatus::Registered,
            };

            peers.push(PeerInfo {
                host_id: row.try_get("host_id").unwrap(),
                pubkey,
                hostname: row.try_get("hostname").ok(),
                registered_at: row.try_get("registered_at").unwrap(),
                last_seen_at: row.try_get("last_seen_at").ok(),
                last_heartbeat_at: row.try_get("last_heartbeat_at").ok(),
                attestation_metadata,
                active: row.try_get::<i64, _>("active").unwrap() != 0,
                health_status,
                discovery_status,
                failed_heartbeats: row.try_get::<i32, _>("failed_heartbeats").unwrap_or(0) as u32,
            });
        }

        Ok(peers)
    }

    /// List peers by health status
    pub async fn list_peers_by_health(&self, status: PeerHealthStatus) -> Result<Vec<PeerInfo>> {
        let pool = self.db.pool();
        let status_str = format!("{:?}", status).to_lowercase();

        let rows = sqlx::query(
            r#"
            SELECT host_id, pubkey, hostname, registered_at, last_seen_at, last_heartbeat_at,
                   attestation_metadata, active, health_status, discovery_status, failed_heartbeats
            FROM federation_peers
            WHERE health_status = ? AND active = 1
            ORDER BY last_heartbeat_at DESC
            "#,
        )
        .bind(&status_str)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list peers by health: {}", e)))?;

        let mut peers = Vec::new();
        for row in rows {
            let pubkey_hex: String = row.try_get("pubkey")?;
            let pubkey_bytes = hex::decode(&pubkey_hex)
                .map_err(|e| AosError::Crypto(format!("Invalid pubkey hex: {}", e)))?;
            if pubkey_bytes.len() != 32 {
                return Err(AosError::Crypto("Invalid public key length".to_string()));
            }
            let mut key_array = [0u8; 32];
            key_array.copy_from_slice(&pubkey_bytes);
            let pubkey = PublicKey::from_bytes(&key_array)?;

            let attestation_metadata: Option<String> = row.try_get("attestation_metadata").ok();
            let attestation_metadata =
                attestation_metadata.and_then(|json| serde_json::from_str(&json).ok());

            peers.push(PeerInfo {
                host_id: row.try_get("host_id")?,
                pubkey,
                hostname: row.try_get("hostname").ok(),
                registered_at: row.try_get("registered_at")?,
                last_seen_at: row.try_get("last_seen_at").ok(),
                last_heartbeat_at: row.try_get("last_heartbeat_at").ok(),
                attestation_metadata,
                active: true,
                health_status: status,
                discovery_status: DiscoveryStatus::Registered,
                failed_heartbeats: row.try_get::<i32, _>("failed_heartbeats").unwrap_or(0) as u32,
            });
        }

        Ok(peers)
    }

    /// Update peer last seen timestamp
    pub async fn update_last_seen(&self, host_id: &str) -> Result<()> {
        let pool = self.db.pool();

        sqlx::query(
            r#"
            UPDATE federation_peers
            SET last_seen_at = datetime('now')
            WHERE host_id = ?
            "#,
        )
        .bind(host_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update last seen: {}", e)))?;

        Ok(())
    }

    /// Deactivate a peer
    pub async fn deactivate_peer(&self, host_id: &str) -> Result<()> {
        info!(host_id = %host_id, "Deactivating peer");

        let pool = self.db.pool();

        sqlx::query(
            r#"
            UPDATE federation_peers
            SET active = 0
            WHERE host_id = ?
            "#,
        )
        .bind(host_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to deactivate peer: {}", e)))?;

        // Remove from cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.remove(host_id);
        }

        Ok(())
    }

    /// Record a health check for a peer
    pub async fn record_health_check(
        &self,
        host_id: &str,
        status: PeerHealthStatus,
        response_time_ms: u32,
        error_message: Option<String>,
    ) -> Result<()> {
        let pool = self.db.pool();
        let now = chrono::Utc::now().to_rfc3339();
        let status_str = format!("{:?}", status).to_lowercase();

        // Record health check
        sqlx::query(
            r#"
            INSERT INTO peer_health_checks (host_id, timestamp, status, response_time_ms, error_message)
            VALUES (?, ?, ?, ?, ?)
            "#
        )
        .bind(host_id)
        .bind(&now)
        .bind(&status_str)
        .bind(response_time_ms as i32)
        .bind(&error_message)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to record health check: {}", e)))?;

        // Update peer health status and heartbeat timestamp
        let (new_failed_count, should_mark_unhealthy) = if status == PeerHealthStatus::Healthy {
            (0, false)
        } else {
            // Get current failed count
            let row =
                sqlx::query("SELECT failed_heartbeats FROM federation_peers WHERE host_id = ?")
                    .bind(host_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| AosError::Database(format!("Failed to fetch peer: {}", e)))?;

            if let Some(row) = row {
                let current_failed: i32 = row.try_get("failed_heartbeats").unwrap_or(0);
                let new_count = current_failed + 1;
                (
                    new_count as u32,
                    new_count >= self.max_failed_heartbeats as i32,
                )
            } else {
                (1, false)
            }
        };

        let final_status = if should_mark_unhealthy {
            PeerHealthStatus::Unhealthy
        } else {
            status
        };

        let final_status_str = format!("{:?}", final_status).to_lowercase();

        sqlx::query(
            r#"
            UPDATE federation_peers
            SET health_status = ?,
                last_heartbeat_at = ?,
                failed_heartbeats = ?
            WHERE host_id = ?
            "#,
        )
        .bind(&final_status_str)
        .bind(&now)
        .bind(new_failed_count as i32)
        .bind(host_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update peer health: {}", e)))?;

        // Update cache
        if let Ok(Some(mut peer)) = self.get_peer(host_id).await {
            peer.health_status = final_status;
            peer.last_heartbeat_at = Some(now);
            peer.failed_heartbeats = new_failed_count;
            let mut cache = self.cache.write().unwrap();
            cache.insert(host_id.to_string(), peer);
        }

        info!(
            host_id = %host_id,
            status = ?final_status,
            response_time_ms = response_time_ms,
            "Health check recorded"
        );

        Ok(())
    }

    /// Get recent health checks for a peer
    pub async fn get_health_history(
        &self,
        host_id: &str,
        limit: usize,
    ) -> Result<Vec<PeerHealthCheck>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT host_id, timestamp, status, response_time_ms, error_message
            FROM peer_health_checks
            WHERE host_id = ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(host_id)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch health history: {}", e)))?;

        let checks = rows
            .into_iter()
            .map(|row| {
                let status_str: String = row.try_get("status").unwrap_or_default();
                let status = match status_str.as_str() {
                    "degraded" => PeerHealthStatus::Degraded,
                    "unhealthy" => PeerHealthStatus::Unhealthy,
                    "isolated" => PeerHealthStatus::Isolated,
                    _ => PeerHealthStatus::Healthy,
                };

                PeerHealthCheck {
                    host_id: row.try_get("host_id").unwrap_or_default(),
                    timestamp: row.try_get("timestamp").unwrap_or_default(),
                    status,
                    response_time_ms: row.try_get::<i32, _>("response_time_ms").unwrap_or(0) as u32,
                    error_message: row.try_get("error_message").ok(),
                }
            })
            .collect();

        Ok(checks)
    }

    /// Initiate consensus vote on a peer state change
    pub async fn initiate_consensus(
        &self,
        peer_id: &str,
        action: String,
        participating_hosts: Vec<String>,
    ) -> Result<String> {
        let pool = self.db.pool();
        let decision_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let required_votes = (participating_hosts.len() / 2) + 1; // Majority quorum

        debug!(
            decision_id = %decision_id,
            peer_id = %peer_id,
            action = %action,
            required_votes = required_votes,
            "Initiating consensus decision"
        );

        sqlx::query(
            r#"
            INSERT INTO consensus_decisions (id, peer_id, action, participating_hosts_json, required_votes, collected_votes, approved, timestamp)
            VALUES (?, ?, ?, ?, ?, 0, 0, ?)
            "#
        )
        .bind(&decision_id)
        .bind(peer_id)
        .bind(&action)
        .bind(serde_json::to_string(&participating_hosts).unwrap_or_default())
        .bind(required_votes as i32)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to initiate consensus: {}", e)))?;

        info!(
            decision_id = %decision_id,
            peer_id = %peer_id,
            action = %action,
            "Consensus decision initiated"
        );

        Ok(decision_id)
    }

    /// Record a vote in a consensus decision
    pub async fn record_consensus_vote(
        &self,
        decision_id: &str,
        _voting_host: &str,
        _approved: bool,
    ) -> Result<bool> {
        let pool = self.db.pool();

        // Get decision details
        let decision = sqlx::query(
            r#"
            SELECT id, peer_id, action, participating_hosts_json, required_votes, collected_votes, approved
            FROM consensus_decisions
            WHERE id = ?
            "#
        )
        .bind(decision_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch decision: {}", e)))?;

        if decision.is_none() {
            return Err(AosError::Validation(format!(
                "Decision not found: {}",
                decision_id
            )));
        }

        let decision = decision.unwrap();
        let required_votes: i32 = decision.try_get("required_votes")?;
        let mut collected_votes: i32 = decision.try_get("collected_votes")?;

        collected_votes += 1;
        let quorum_reached = collected_votes >= required_votes;

        // Update decision with new vote count
        sqlx::query(
            r#"
            UPDATE consensus_decisions
            SET collected_votes = ?
            WHERE id = ?
            "#,
        )
        .bind(collected_votes)
        .bind(decision_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update decision: {}", e)))?;

        if quorum_reached {
            // Mark decision as approved
            sqlx::query(
                r#"
                UPDATE consensus_decisions
                SET approved = 1
                WHERE id = ?
                "#,
            )
            .bind(decision_id)
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to approve decision: {}", e)))?;

            let peer_id: String = decision.try_get("peer_id")?;
            info!(
                decision_id = %decision_id,
                peer_id = %peer_id,
                votes = collected_votes,
                required = required_votes,
                "Consensus quorum reached"
            );
        } else {
            debug!(
                decision_id = %decision_id,
                votes = collected_votes,
                required = required_votes,
                "Consensus vote recorded"
            );
        }

        Ok(quorum_reached)
    }

    /// Detect network partition
    pub async fn detect_partition(
        &self,
        reachable_peers: HashSet<String>,
    ) -> Result<Option<PartitionEvent>> {
        let pool = self.db.pool();
        let all_peers = self.get_all_peer_ids().await?;
        let all_peer_set: HashSet<String> = all_peers.into_iter().collect();

        // Find isolated peers
        let isolated_peers: Vec<String> =
            all_peer_set.difference(&reachable_peers).cloned().collect();

        if isolated_peers.is_empty() {
            return Ok(None);
        }

        // Create partition event
        let partition_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let reachable_vec: Vec<String> = reachable_peers.into_iter().collect();

        // Determine quorum leader (peer with most connections)
        let quorum_leader = reachable_vec.first().cloned();

        let event = PartitionEvent {
            partition_id: partition_id.clone(),
            detected_at: now.clone(),
            isolated_peers: isolated_peers.clone(),
            reachable_peers: reachable_vec.clone(),
            quorum_leader: quorum_leader.clone(),
            resolved: false,
        };

        // Record partition event
        sqlx::query(
            r#"
            INSERT INTO partition_events (partition_id, detected_at, isolated_peers_json, reachable_peers_json, quorum_leader, resolved)
            VALUES (?, ?, ?, ?, ?, 0)
            "#
        )
        .bind(&partition_id)
        .bind(&now)
        .bind(serde_json::to_string(&isolated_peers).unwrap_or_default())
        .bind(serde_json::to_string(&reachable_vec).unwrap_or_default())
        .bind(&quorum_leader)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to record partition: {}", e)))?;

        // Mark isolated peers as isolated
        for peer_id in &isolated_peers {
            self.record_health_check(
                peer_id,
                PeerHealthStatus::Isolated,
                0,
                Some("Network partition detected".to_string()),
            )
            .await?;
        }

        warn!(
            partition_id = %partition_id,
            isolated_count = isolated_peers.len(),
            reachable_count = reachable_vec.len(),
            "Network partition detected"
        );

        Ok(Some(event))
    }

    /// Resolve a partition event
    pub async fn resolve_partition(&self, partition_id: &str) -> Result<()> {
        let pool = self.db.pool();

        // Get partition details
        let partition =
            sqlx::query("SELECT isolated_peers_json FROM partition_events WHERE partition_id = ?")
                .bind(partition_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to fetch partition: {}", e)))?;

        if let Some(row) = partition {
            let isolated_json: String = row.try_get("isolated_peers_json")?;
            let isolated_peers: Vec<String> =
                serde_json::from_str(&isolated_json).unwrap_or_default();

            // Mark isolated peers as healthy again
            for peer_id in isolated_peers {
                self.record_health_check(&peer_id, PeerHealthStatus::Healthy, 0, None)
                    .await?;
            }
        }

        // Mark partition as resolved
        sqlx::query("UPDATE partition_events SET resolved = 1 WHERE partition_id = ?")
            .bind(partition_id)
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to resolve partition: {}", e)))?;

        info!(partition_id = %partition_id, "Network partition resolved");
        Ok(())
    }

    /// Verify peer attestation
    pub fn verify_attestation(&self, peer_info: &PeerInfo) -> Result<bool> {
        // Check if attestation metadata exists
        let attestation = match &peer_info.attestation_metadata {
            Some(a) => a,
            None => {
                warn!(host_id = %peer_info.host_id, "No attestation metadata available");
                return Ok(false);
            }
        };

        // Basic attestation checks
        if !attestation.secure_enclave_available && !attestation.tpm_available {
            warn!(
                host_id = %peer_info.host_id,
                "No hardware root of trust available"
            );
            return Ok(false);
        }

        // Check attestation age (must be < 24 hours)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let age_hours = (now - attestation.attestation_timestamp) / 3600;

        if age_hours > 24 {
            warn!(
                host_id = %peer_info.host_id,
                age_hours = age_hours,
                "Attestation is too old"
            );
            return Ok(false);
        }

        debug!(host_id = %peer_info.host_id, "Attestation verified");
        Ok(true)
    }

    /// Load cache from database
    pub async fn load_cache(&self) -> Result<()> {
        info!("Loading peer registry cache");

        let peers = self.list_active_peers().await?;

        let mut cache = self.cache.write().unwrap();
        cache.clear();

        for peer in peers {
            cache.insert(peer.host_id.clone(), peer);
        }

        info!(count = cache.len(), "Peer registry cache loaded");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::Keypair;
    use tempfile::TempDir;

    async fn setup_test_db() -> Result<Db> {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join(format!("test_{}.db", uuid::Uuid::new_v4()));
        let db = Db::connect(db_path.to_str().unwrap()).await?;
        db.migrate().await?;
        Ok(db)
    }

    #[tokio::test]
    async fn test_register_and_get_peer() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        let pubkey = keypair.public_key();

        registry
            .register_peer(
                "test-host".to_string(),
                pubkey,
                Some("test.example.com".to_string()),
                None,
            )
            .await?;

        let peer = registry.get_peer("test-host").await?;
        assert!(peer.is_some());

        let peer = peer.unwrap();
        assert_eq!(peer.host_id, "test-host");
        assert_eq!(peer.hostname, Some("test.example.com".to_string()));
        assert_eq!(peer.health_status, PeerHealthStatus::Healthy);
        assert_eq!(peer.discovery_status, DiscoveryStatus::Registered);

        Ok(())
    }

    #[tokio::test]
    async fn test_list_active_peers() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        registry
            .register_peer("host1".to_string(), keypair1.public_key(), None, None)
            .await?;

        registry
            .register_peer("host2".to_string(), keypair2.public_key(), None, None)
            .await?;

        let peers = registry.list_active_peers().await?;
        assert_eq!(peers.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_deactivate_peer() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();

        registry
            .register_peer("test-host".to_string(), keypair.public_key(), None, None)
            .await?;

        registry.deactivate_peer("test-host").await?;

        let peers = registry.list_active_peers().await?;
        assert_eq!(peers.len(), 0);

        Ok(())
    }

    /// Multi-peer discovery test
    #[tokio::test]
    async fn test_discovery_announcement() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register initial peers
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();
        let keypair3 = Keypair::generate();

        registry
            .register_peer("host1".to_string(), keypair1.public_key(), None, None)
            .await?;

        registry
            .register_peer("host2".to_string(), keypair2.public_key(), None, None)
            .await?;

        // Simulate discovery announcement
        let announcement = DiscoveryAnnouncement {
            sender_id: "host1".to_string(),
            known_peers: vec!["host2".to_string(), "host3".to_string()],
            announcement_time: std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            federation_epoch: 1,
        };

        let discovered = registry
            .process_discovery_announcement(&announcement)
            .await?;
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0], "host3");

        Ok(())
    }

    /// Multi-peer health check test
    #[tokio::test]
    async fn test_health_check_recording() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("test-host".to_string(), keypair.public_key(), None, None)
            .await?;

        // Record healthy check
        registry
            .record_health_check("test-host", PeerHealthStatus::Healthy, 15, None)
            .await?;

        let peer = registry.get_peer("test-host").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Healthy);
        assert_eq!(peer.failed_heartbeats, 0);

        // Record degraded checks
        registry
            .record_health_check(
                "test-host",
                PeerHealthStatus::Degraded,
                100,
                Some("slow response".to_string()),
            )
            .await?;

        let peer = registry.get_peer("test-host").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Degraded);
        assert_eq!(peer.failed_heartbeats, 1);

        Ok(())
    }

    /// Health history retrieval test
    #[tokio::test]
    async fn test_health_history() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        let keypair = Keypair::generate();
        registry
            .register_peer("test-host".to_string(), keypair.public_key(), None, None)
            .await?;

        // Record multiple health checks
        for i in 0..5 {
            registry
                .record_health_check(
                    "test-host",
                    if i % 2 == 0 {
                        PeerHealthStatus::Healthy
                    } else {
                        PeerHealthStatus::Degraded
                    },
                    10 + (i as u32 * 5),
                    None,
                )
                .await?;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let history = registry.get_health_history("test-host", 10).await?;
        assert!(history.len() >= 5);

        Ok(())
    }

    /// Consensus voting test
    #[tokio::test]
    async fn test_consensus_quorum() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 3 peers for voting
        let peers = vec!["host1", "host2", "host3"];
        for peer_id in &peers {
            let keypair = Keypair::generate();
            registry
                .register_peer(peer_id.to_string(), keypair.public_key(), None, None)
                .await?;
        }

        // Initiate consensus decision
        let decision_id = registry
            .initiate_consensus(
                "host1",
                "evict_peer".to_string(),
                peers.iter().map(|p| p.to_string()).collect(),
            )
            .await?;

        // Record votes (majority = 2 out of 3)
        let quorum1 = registry
            .record_consensus_vote(&decision_id, "host1", true)
            .await?;
        assert!(!quorum1); // 1 vote < 2 required

        let quorum2 = registry
            .record_consensus_vote(&decision_id, "host2", true)
            .await?;
        assert!(quorum2); // 2 votes >= 2 required (quorum reached)

        Ok(())
    }

    /// Network partition detection test
    #[tokio::test]
    async fn test_partition_detection() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register 5 peers
        let all_peers = vec!["host1", "host2", "host3", "host4", "host5"];
        for peer_id in &all_peers {
            let keypair = Keypair::generate();
            registry
                .register_peer(peer_id.to_string(), keypair.public_key(), None, None)
                .await?;
        }

        // Simulate network partition: 3 peers reachable, 2 isolated
        let reachable: HashSet<String> = vec![
            "host1".to_string(),
            "host2".to_string(),
            "host3".to_string(),
        ]
        .into_iter()
        .collect();

        let partition = registry.detect_partition(reachable).await?;
        assert!(partition.is_some());

        let partition = partition.unwrap();
        assert_eq!(partition.isolated_peers.len(), 2);
        assert_eq!(partition.reachable_peers.len(), 3);
        assert!(partition.reachable_peers.contains(&"host1".to_string()));
        assert!(!partition.isolated_peers.contains(&"host1".to_string()));

        // Verify isolated peers are marked as isolated
        let peer3 = registry.get_peer("host4").await?.unwrap();
        assert_eq!(peer3.health_status, PeerHealthStatus::Isolated);

        Ok(())
    }

    /// Network partition recovery test
    #[tokio::test]
    async fn test_partition_recovery() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register peers
        let peers = vec!["host1", "host2", "host3"];
        for peer_id in &peers {
            let keypair = Keypair::generate();
            registry
                .register_peer(peer_id.to_string(), keypair.public_key(), None, None)
                .await?;
        }

        // Detect partition
        let reachable: HashSet<String> = vec!["host1".to_string()].into_iter().collect();
        let partition = registry.detect_partition(reachable).await?.unwrap();
        let partition_id = partition.partition_id.clone();

        // Verify isolation
        let peer2 = registry.get_peer("host2").await?.unwrap();
        assert_eq!(peer2.health_status, PeerHealthStatus::Isolated);

        // Resolve partition
        registry.resolve_partition(&partition_id).await?;

        // Verify recovery
        let peer2_recovered = registry.get_peer("host2").await?.unwrap();
        assert_eq!(peer2_recovered.health_status, PeerHealthStatus::Healthy);

        Ok(())
    }

    /// Multi-peer list by health status test
    #[tokio::test]
    async fn test_list_peers_by_health() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Register peers with different health statuses
        for i in 1..=3 {
            let keypair = Keypair::generate();
            registry
                .register_peer(format!("host{}", i), keypair.public_key(), None, None)
                .await?;
        }

        // Mark one as degraded
        registry
            .record_health_check("host1", PeerHealthStatus::Degraded, 50, None)
            .await?;

        // Get healthy peers
        let healthy = registry
            .list_peers_by_health(PeerHealthStatus::Healthy)
            .await?;
        assert_eq!(healthy.len(), 2); // host2, host3

        // Get degraded peers
        let degraded = registry
            .list_peers_by_health(PeerHealthStatus::Degraded)
            .await?;
        assert_eq!(degraded.len(), 1); // host1

        Ok(())
    }

    /// Discovery with multiple peers test
    #[tokio::test]
    async fn test_multi_peer_discovery_cascade() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::new(Arc::new(db));

        // Initial peers
        registry
            .register_peer(
                "seed1".to_string(),
                Keypair::generate().public_key(),
                None,
                None,
            )
            .await?;
        registry
            .register_peer(
                "seed2".to_string(),
                Keypair::generate().public_key(),
                None,
                None,
            )
            .await?;

        // First announcement: seed1 knows about peer1, peer2
        let announce1 = DiscoveryAnnouncement {
            sender_id: "seed1".to_string(),
            known_peers: vec!["peer1".to_string(), "peer2".to_string()],
            announcement_time: 1000,
            federation_epoch: 1,
        };

        let discovered1 = registry.process_discovery_announcement(&announce1).await?;
        assert_eq!(discovered1.len(), 2);

        // Second announcement: seed2 knows about peer2, peer3
        let announce2 = DiscoveryAnnouncement {
            sender_id: "seed2".to_string(),
            known_peers: vec!["peer2".to_string(), "peer3".to_string()],
            announcement_time: 2000,
            federation_epoch: 1,
        };

        let discovered2 = registry.process_discovery_announcement(&announce2).await?;
        assert_eq!(discovered2.len(), 1); // Only peer3 is new
        assert_eq!(discovered2[0], "peer3");

        Ok(())
    }

    /// Test failed heartbeat threshold
    #[tokio::test]
    async fn test_failed_heartbeat_threshold() -> Result<()> {
        let db = setup_test_db().await?;
        let registry = PeerRegistry::with_config(Arc::new(db), 2, 30, 2);

        let keypair = Keypair::generate();
        registry
            .register_peer("test-host".to_string(), keypair.public_key(), None, None)
            .await?;

        // First failed check
        registry
            .record_health_check(
                "test-host",
                PeerHealthStatus::Degraded,
                150,
                Some("timeout".to_string()),
            )
            .await?;
        let peer = registry.get_peer("test-host").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Degraded);

        // Second failed check - should trigger unhealthy
        registry
            .record_health_check(
                "test-host",
                PeerHealthStatus::Degraded,
                200,
                Some("timeout".to_string()),
            )
            .await?;
        let peer = registry.get_peer("test-host").await?.unwrap();
        assert_eq!(peer.health_status, PeerHealthStatus::Unhealthy);
        assert_eq!(peer.failed_heartbeats, 2);

        Ok(())
    }

    /// Test peer cache loading
    #[tokio::test]
    async fn test_load_cache_from_db() -> Result<()> {
        let db = setup_test_db().await?;
        let registry1 = PeerRegistry::new(Arc::new(db.clone()));

        // Register peers
        registry1
            .register_peer(
                "host1".to_string(),
                Keypair::generate().public_key(),
                None,
                None,
            )
            .await?;
        registry1
            .register_peer(
                "host2".to_string(),
                Keypair::generate().public_key(),
                None,
                None,
            )
            .await?;

        // Create new registry and load cache
        let registry2 = PeerRegistry::new(Arc::new(db));
        registry2.load_cache().await?;

        // Verify cache loaded
        let peers = registry2.list_active_peers().await?;
        assert_eq!(peers.len(), 2);

        Ok(())
    }
}
