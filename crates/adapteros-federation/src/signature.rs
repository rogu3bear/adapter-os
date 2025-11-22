//! Bundle Signature Exchange - Quorum-based verification
//!
//! Implements multi-host signature collection and quorum verification

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{PublicKey, Signature};
use adapteros_db::Db;
use adapteros_telemetry::{LogLevel, TelemetryEventBuilder, TelemetryWriter};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Bundle signature exchange for quorum-based verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleSignatureExchange {
    pub bundle_hash: B3Hash,
    pub signatures: HashMap<String, Signature>, // host_id -> signature
    pub quorum_threshold: usize,
}

impl BundleSignatureExchange {
    /// Create a new signature exchange
    pub fn new(bundle_hash: B3Hash, quorum_threshold: usize) -> Self {
        Self {
            bundle_hash,
            signatures: HashMap::new(),
            quorum_threshold,
        }
    }

    /// Add a signature from a host
    pub fn add_signature(&mut self, host_id: String, signature: Signature) {
        self.signatures.insert(host_id, signature);
    }

    /// Check if quorum is reached
    pub fn is_quorum_reached(&self) -> bool {
        self.signatures.len() >= self.quorum_threshold
    }

    /// Get signature count
    pub fn signature_count(&self) -> usize {
        self.signatures.len()
    }

    /// Get participating hosts
    pub fn hosts(&self) -> Vec<String> {
        self.signatures.keys().cloned().collect()
    }
}

/// Quorum manager for bundle signature coordination
pub struct QuorumManager {
    db: Arc<Db>,
    telemetry: Option<Arc<TelemetryWriter>>,
}

impl QuorumManager {
    /// Create a new quorum manager
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            telemetry: None,
        }
    }

    /// Create with telemetry writer
    pub fn with_telemetry(db: Arc<Db>, telemetry: Arc<TelemetryWriter>) -> Self {
        Self {
            db,
            telemetry: Some(telemetry),
        }
    }

    /// Initialize quorum tracking for a bundle
    pub async fn init_quorum(
        &self,
        bundle_hash: &B3Hash,
        required_signatures: usize,
    ) -> Result<()> {
        info!(
            bundle_hash = %bundle_hash.to_hex(),
            required_signatures = required_signatures,
            "Initializing quorum tracking"
        );

        let pool = self.db.pool();
        let bundle_hash_hex = bundle_hash.to_hex();

        sqlx::query(
            r#"
            INSERT INTO federation_bundle_quorum (bundle_hash, required_signatures, collected_signatures, quorum_reached)
            VALUES (?, ?, 0, 0)
            ON CONFLICT(bundle_hash) DO UPDATE SET
                required_signatures = excluded.required_signatures
            "#
        )
        .bind(&bundle_hash_hex)
        .bind(required_signatures as i64)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to initialize quorum: {}", e)))?;

        Ok(())
    }

    /// Record a signature and update quorum status
    pub async fn record_signature(
        &self,
        bundle_hash: &B3Hash,
        host_id: &str,
        signature: &Signature,
    ) -> Result<bool> {
        debug!(
            bundle_hash = %bundle_hash.to_hex(),
            host_id = %host_id,
            "Recording signature"
        );

        let pool = self.db.pool();
        let bundle_hash_hex = bundle_hash.to_hex();
        let signature_hex = hex::encode(signature.to_bytes());

        // Insert signature into federation_bundle_signatures table
        sqlx::query(
            r#"
            INSERT INTO federation_bundle_signatures (host_id, bundle_hash, signature, prev_host_hash, created_at, verified)
            VALUES (?, ?, ?, NULL, datetime('now'), 0)
            "#
        )
        .bind(host_id)
        .bind(&bundle_hash_hex)
        .bind(&signature_hex)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to record signature: {}", e)))?;

        // Update quorum count
        let result = sqlx::query(
            r#"
            UPDATE federation_bundle_quorum
            SET collected_signatures = collected_signatures + 1
            WHERE bundle_hash = ?
            RETURNING required_signatures, collected_signatures
            "#,
        )
        .bind(&bundle_hash_hex)
        .fetch_one(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update quorum: {}", e)))?;

        let required: i64 = result.try_get("required_signatures")?;
        let collected: i64 = result.try_get("collected_signatures")?;
        let quorum_reached = collected >= required;

        // If quorum reached, update status
        if quorum_reached {
            sqlx::query(
                r#"
                UPDATE federation_bundle_quorum
                SET quorum_reached = 1,
                    quorum_reached_at = datetime('now')
                WHERE bundle_hash = ?
                "#,
            )
            .bind(&bundle_hash_hex)
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to mark quorum reached: {}", e)))?;

            // Emit telemetry event (100% sampling)
            if let Some(ref telemetry) = self.telemetry {
                let identity = adapteros_core::identity::IdentityEnvelope::new(
                    "system".to_string(),
                    "federation".to_string(),
                    "quorum".to_string(),
                    "1.0".to_string(),
                );
                match TelemetryEventBuilder::new(
                    adapteros_telemetry::EventType::Custom("federation.quorum_reached".to_string()),
                    LogLevel::Info,
                    format!("Quorum reached for bundle: {}", bundle_hash.to_hex()),
                    identity,
                )
                .component("adapteros-federation".to_string())
                .metadata(json!({
                    "bundle_hash": bundle_hash.to_hex(),
                    "required_signatures": required,
                    "collected_signatures": collected,
                }))
                .build()
                {
                    Ok(event) => {
                        let _ = telemetry.log_event(event);
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to build telemetry event for quorum reached");
                    }
                }
            }

            info!(
                bundle_hash = %bundle_hash.to_hex(),
                required = required,
                collected = collected,
                "Quorum reached"
            );
        }

        Ok(quorum_reached)
    }

    /// Check if quorum is reached for a bundle
    pub async fn is_quorum_reached(&self, bundle_hash: &B3Hash) -> Result<bool> {
        let pool = self.db.pool();
        let bundle_hash_hex = bundle_hash.to_hex();

        let row = sqlx::query(
            r#"
            SELECT quorum_reached
            FROM federation_bundle_quorum
            WHERE bundle_hash = ?
            "#,
        )
        .bind(&bundle_hash_hex)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check quorum: {}", e)))?;

        if let Some(row) = row {
            let reached: i64 = row.try_get("quorum_reached")?;
            Ok(reached != 0)
        } else {
            Ok(false)
        }
    }

    /// Get quorum status for a bundle
    pub async fn get_quorum_status(&self, bundle_hash: &B3Hash) -> Result<QuorumStatus> {
        let pool = self.db.pool();
        let bundle_hash_hex = bundle_hash.to_hex();

        let row = sqlx::query(
            r#"
            SELECT required_signatures, collected_signatures, quorum_reached, created_at, quorum_reached_at
            FROM federation_bundle_quorum
            WHERE bundle_hash = ?
            "#
        )
        .bind(&bundle_hash_hex)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get quorum status: {}", e)))?;

        if let Some(row) = row {
            let required: i64 = row.try_get("required_signatures")?;
            let collected: i64 = row.try_get("collected_signatures")?;
            let reached: i64 = row.try_get("quorum_reached")?;

            Ok(QuorumStatus {
                bundle_hash: *bundle_hash,
                required_signatures: required as usize,
                collected_signatures: collected as usize,
                quorum_reached: reached != 0,
                created_at: row.try_get("created_at")?,
                quorum_reached_at: row.try_get("quorum_reached_at").ok(),
            })
        } else {
            Err(AosError::Validation(format!(
                "No quorum tracking found for bundle: {}",
                bundle_hash.to_hex()
            )))
        }
    }

    /// Build signature exchange from database
    pub async fn build_exchange(&self, bundle_hash: &B3Hash) -> Result<BundleSignatureExchange> {
        let status = self.get_quorum_status(bundle_hash).await?;
        let pool = self.db.pool();
        let bundle_hash_hex = bundle_hash.to_hex();

        // Fetch all signatures for this bundle
        let rows = sqlx::query(
            r#"
            SELECT host_id, signature
            FROM federation_bundle_signatures
            WHERE bundle_hash = ?
            "#,
        )
        .bind(&bundle_hash_hex)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch signatures: {}", e)))?;

        let mut exchange = BundleSignatureExchange::new(*bundle_hash, status.required_signatures);

        for row in rows {
            let host_id: String = row.try_get("host_id")?;
            let signature_hex: String = row.try_get("signature")?;
            let signature_bytes = hex::decode(&signature_hex)
                .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

            if signature_bytes.len() == 64 {
                let mut sig_array = [0u8; 64];
                sig_array.copy_from_slice(&signature_bytes);
                let signature = Signature::from_bytes(&sig_array)?;
                exchange.add_signature(host_id, signature);
            }
        }

        Ok(exchange)
    }

    /// Verify all signatures in an exchange
    pub fn verify_exchange(
        &self,
        exchange: &BundleSignatureExchange,
        host_pubkeys: &HashMap<String, PublicKey>,
        message: &[u8],
    ) -> Result<VerificationResult> {
        let mut verified_hosts = Vec::new();
        let mut failed_hosts = Vec::new();

        for (host_id, signature) in &exchange.signatures {
            if let Some(pubkey) = host_pubkeys.get(host_id) {
                match pubkey.verify(message, signature) {
                    Ok(_) => verified_hosts.push(host_id.clone()),
                    Err(_) => failed_hosts.push(host_id.clone()),
                }
            } else {
                warn!(host_id = %host_id, "No public key found for host");
                failed_hosts.push(host_id.clone());
            }
        }

        let all_verified = failed_hosts.is_empty();
        let quorum_verified = verified_hosts.len() >= exchange.quorum_threshold;

        Ok(VerificationResult {
            all_verified,
            quorum_verified,
            verified_hosts,
            failed_hosts,
            total_signatures: exchange.signature_count(),
            quorum_threshold: exchange.quorum_threshold,
        })
    }
}

/// Quorum status for a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumStatus {
    pub bundle_hash: B3Hash,
    pub required_signatures: usize,
    pub collected_signatures: usize,
    pub quorum_reached: bool,
    pub created_at: String,
    pub quorum_reached_at: Option<String>,
}

/// Verification result for a signature exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub all_verified: bool,
    pub quorum_verified: bool,
    pub verified_hosts: Vec<String>,
    pub failed_hosts: Vec<String>,
    pub total_signatures: usize,
    pub quorum_threshold: usize,
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
    async fn test_quorum_init_and_check() -> Result<()> {
        let db = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        manager.init_quorum(&bundle_hash, 2).await?;

        let status = manager.get_quorum_status(&bundle_hash).await?;
        assert_eq!(status.required_signatures, 2);
        assert_eq!(status.collected_signatures, 0);
        assert!(!status.quorum_reached);

        Ok(())
    }

    #[tokio::test]
    async fn test_quorum_signature_recording() -> Result<()> {
        let db = setup_test_db().await?;
        let manager = QuorumManager::new(Arc::new(db));

        let bundle_hash = B3Hash::hash(b"test bundle");
        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        manager.init_quorum(&bundle_hash, 2).await?;

        let message = b"test message";
        let sig1 = keypair1.sign(message);
        let sig2 = keypair2.sign(message);

        // First signature
        let reached = manager
            .record_signature(&bundle_hash, "host1", &sig1)
            .await?;
        assert!(!reached);

        // Second signature - reaches quorum
        let reached = manager
            .record_signature(&bundle_hash, "host2", &sig2)
            .await?;
        assert!(reached);

        let is_reached = manager.is_quorum_reached(&bundle_hash).await?;
        assert!(is_reached);

        Ok(())
    }

    #[test]
    fn test_signature_exchange() {
        let bundle_hash = B3Hash::hash(b"test bundle");
        let mut exchange = BundleSignatureExchange::new(bundle_hash, 2);

        let keypair1 = Keypair::generate();
        let keypair2 = Keypair::generate();

        let message = b"test message";
        let sig1 = keypair1.sign(message);
        let sig2 = keypair2.sign(message);

        exchange.add_signature("host1".to_string(), sig1);
        assert_eq!(exchange.signature_count(), 1);
        assert!(!exchange.is_quorum_reached());

        exchange.add_signature("host2".to_string(), sig2);
        assert_eq!(exchange.signature_count(), 2);
        assert!(exchange.is_quorum_reached());
    }
}
