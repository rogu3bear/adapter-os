//! Peer Registry - Management of federated hosts
//!
//! Provides peer registration, lookup, and attestation verification

use adapteros_core::{AosError, Result};
use adapteros_crypto::PublicKey;
use adapteros_db::Db;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Peer information for a federated host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub host_id: String,
    pub pubkey: PublicKey,
    pub hostname: Option<String>,
    pub registered_at: String,
    pub last_seen_at: Option<String>,
    pub attestation_metadata: Option<AttestationMetadata>,
    pub active: bool,
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

/// Peer registry for managing federated hosts
pub struct PeerRegistry {
    db: Arc<Db>,
    cache: Arc<RwLock<HashMap<String, PeerInfo>>>,
}

impl PeerRegistry {
    /// Create a new peer registry
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
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

        // Insert or update peer
        sqlx::query(
            r#"
            INSERT INTO federation_peers (host_id, pubkey, hostname, attestation_metadata, registered_at, active)
            VALUES (?, ?, ?, ?, datetime('now'), 1)
            ON CONFLICT(host_id) DO UPDATE SET
                pubkey = excluded.pubkey,
                hostname = excluded.hostname,
                attestation_metadata = excluded.attestation_metadata,
                last_seen_at = datetime('now'),
                active = 1
            "#
        )
        .bind(&host_id)
        .bind(&pubkey_hex)
        .bind(&hostname)
        .bind(&attestation_json)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register peer: {}", e)))?;

        // Update cache
        let peer_info = PeerInfo {
            host_id: host_id.clone(),
            pubkey,
            hostname,
            registered_at: chrono::Utc::now().to_rfc3339(),
            last_seen_at: None,
            attestation_metadata,
            active: true,
        };

        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(host_id.clone(), peer_info);
        }

        info!(host_id = %host_id, "Peer registered successfully");
        Ok(())
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
            SELECT host_id, pubkey, hostname, registered_at, last_seen_at, attestation_metadata, active
            FROM federation_peers
            WHERE host_id = ?
            "#
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

            let peer_info = PeerInfo {
                host_id: row.try_get("host_id").unwrap(),
                pubkey,
                hostname: row.try_get("hostname").ok(),
                registered_at: row.try_get("registered_at").unwrap(),
                last_seen_at: row.try_get("last_seen_at").ok(),
                attestation_metadata,
                active: row.try_get::<i64, _>("active").unwrap() != 0,
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
            SELECT host_id, pubkey, hostname, registered_at, last_seen_at, attestation_metadata, active
            FROM federation_peers
            WHERE active = 1
            ORDER BY last_seen_at DESC
            "#
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

            peers.push(PeerInfo {
                host_id: row.try_get("host_id").unwrap(),
                pubkey,
                hostname: row.try_get("hostname").ok(),
                registered_at: row.try_get("registered_at").unwrap(),
                last_seen_at: row.try_get("last_seen_at").ok(),
                attestation_metadata,
                active: row.try_get::<i64, _>("active").unwrap() != 0,
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
        let db_path = temp_dir.path().join("test.db");
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
}
