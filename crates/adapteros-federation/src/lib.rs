//! Federation - Cross-Host Bundle Signature Chain
//!
//! Implements federated Ed25519 signatures for telemetry bundles across
//! multiple hosts, enabling deterministic cross-host verification and
//! chain validation.
//!
//! ## Features
//!
//! - **Cross-Host Signatures**: Ed25519 signatures for bundle metadata
//! - **Chain Validation**: Verify Merkle chain continuity across hosts
//! - **Secure Enclave Integration**: Optional hardware-backed signing
//! - **Database Storage**: Persistent signature storage with verification status
//!
//! ## Policy Compliance
//!
//! - Determinism Ruleset (#2): Reproducible signature chains
//! - Isolation Ruleset (#8): Per-tenant signature isolation
//! - Telemetry Ruleset (#9): Signed bundle rotation
//! - Artifacts Ruleset (#13): Signature verification gates

pub mod attestation;
pub mod output_hash;
pub mod peer;
pub mod signature;

use adapteros_core::{identity::IdentityEnvelope, AosError, Domain, Purpose, Result};
use adapteros_crypto::{Keypair, PublicKey, Signature};
use adapteros_db::Db;
use adapteros_telemetry::{LogLevel, StoredBundleMetadata, TelemetryEventBuilder, TelemetryWriter};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use tracing::{debug, info, warn};

/// Federation signature record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSignature {
    pub id: Option<String>,
    pub host_id: String,
    pub bundle_hash: String,
    pub signature: String,
    pub prev_host_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub verified: bool,
}

impl FederationSignature {
    /// Create a new federation signature
    pub fn new(
        host_id: String,
        bundle_hash: String,
        signature: String,
        prev_host_hash: Option<String>,
    ) -> Self {
        Self {
            id: None,
            host_id,
            bundle_hash,
            signature,
            prev_host_hash,
            created_at: Utc::now(),
            verified: false,
        }
    }
}

/// Federation Manager - coordinates cross-host signatures and chain validation
pub struct FederationManager {
    db: Db,
    keypair: Keypair,
    host_id: String,
    telemetry: Option<TelemetryWriter>,
}

impl FederationManager {
    /// Create a new federation manager
    ///
    /// # Arguments
    ///
    /// * `db` - Database connection
    /// * `keypair` - Ed25519 keypair for signing
    ///
    /// # Returns
    ///
    /// A new `FederationManager` instance
    pub fn new(db: Db, keypair: Keypair) -> Result<Self> {
        let host_id = Self::get_host_id()?;
        Ok(Self {
            db,
            keypair,
            host_id,
            telemetry: None,
        })
    }

    /// Create a federation manager with telemetry writer
    pub fn with_telemetry(db: Db, keypair: Keypair, telemetry: TelemetryWriter) -> Result<Self> {
        let host_id = Self::get_host_id()?;
        Ok(Self {
            db,
            keypair,
            host_id,
            telemetry: Some(telemetry),
        })
    }

    /// Create a federation manager with a specific host ID (for testing)
    pub fn with_host_id(db: Db, keypair: Keypair, host_id: String) -> Result<Self> {
        Ok(Self {
            db,
            keypair,
            host_id,
            telemetry: None,
        })
    }

    /// Get the local host identifier
    fn get_host_id() -> Result<String> {
        hostname::get()
            .map_err(|e| AosError::Io(format!("Failed to get hostname: {}", e)))?
            .to_string_lossy()
            .into_owned()
            .pipe(Ok)
    }

    /// Get latest tick hash from metadata
    ///
    /// This retrieves the latest tick hash from the deterministic executor
    /// if available, for linking to federation signatures.
    pub fn get_latest_tick_hash(&self) -> Option<String> {
        // This would be populated from DeterministicExecutor if integrated
        None
    }

    /// Sign a telemetry bundle
    ///
    /// Creates a federation signature for the given bundle metadata.
    /// The signature covers the bundle's Merkle root and ensures
    /// cross-host verification.
    ///
    /// # Arguments
    ///
    /// * `metadata` - Bundle metadata to sign
    ///
    /// # Returns
    ///
    /// A `FederationSignature` record
    pub async fn sign_bundle(
        &self,
        metadata: &StoredBundleMetadata,
    ) -> Result<FederationSignature> {
        // Serialize metadata for signing
        let payload = self.serialize_bundle_metadata(metadata)?;

        // Sign with Ed25519
        let signature = self.keypair.sign(&payload);
        let signature_hex = hex::encode(signature.to_bytes());

        // Create signature record
        let record = FederationSignature::new(
            self.host_id.clone(),
            metadata.merkle_root.to_string(),
            signature_hex,
            metadata.prev_bundle_hash.as_ref().map(|h| h.to_string()),
        );

        // Store in database
        self.store_signature(&record).await?;

        // Emit telemetry event (100% sampling per Telemetry Ruleset #9)
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                "system".to_string(),
                Domain::Worker,
                Purpose::Audit,
                IdentityEnvelope::default_revision(),
            );
            let event = TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("federation.bundle_signed".to_string()),
                LogLevel::Info,
                format!("Federation bundle signed: {}", metadata.merkle_root),
                identity,
            )
            .component("adapteros-federation".to_string())
            .metadata(json!({
                "host_id": self.host_id,
                "bundle_hash": metadata.merkle_root.to_string(),
                "signature": &record.signature[..16], // Log first 16 chars only
                "prev_bundle_hash": metadata.prev_bundle_hash.as_ref().map(|h| h.to_string()),
            }))
            .build();

            let _ = telemetry.log_event(event);
        }

        info!(
            host_id = %self.host_id,
            bundle_hash = %metadata.merkle_root,
            "Federation bundle signed"
        );

        Ok(record)
    }

    /// Verify a federation signature
    ///
    /// # Arguments
    ///
    /// * `signature` - Signature to verify
    /// * `public_key` - Public key to verify against
    /// * `metadata` - Bundle metadata
    ///
    /// # Returns
    ///
    /// `true` if signature is valid, `false` otherwise
    pub fn verify_signature(
        &self,
        signature: &FederationSignature,
        public_key: &PublicKey,
        metadata: &StoredBundleMetadata,
    ) -> Result<bool> {
        // Serialize metadata
        let payload = self.serialize_bundle_metadata(metadata)?;

        // Decode signature
        let sig_bytes = hex::decode(&signature.signature)
            .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

        if sig_bytes.len() != 64 {
            return Err(AosError::Crypto("Invalid signature length".to_string()));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        let sig = Signature::from_bytes(&sig_array)?;

        // Verify signature
        match public_key.verify(&payload, &sig) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify cross-host chain continuity
    ///
    /// Validates that a sequence of federation signatures forms
    /// a valid chain with proper prev_host_hash linkage.
    ///
    /// # Arguments
    ///
    /// * `host_chain` - Sequence of signatures to verify
    ///
    /// # Returns
    ///
    /// `Ok(())` if chain is valid, error otherwise
    pub async fn verify_cross_host_chain(&self, host_chain: &[FederationSignature]) -> Result<()> {
        if host_chain.is_empty() {
            return Ok(());
        }

        if host_chain.len() == 1 {
            debug!("Single signature in chain, skipping linkage check");
            return Ok(());
        }

        // Verify chain linkage
        for window in host_chain.windows(2) {
            let prev = &window[0];
            let curr = &window[1];

            // Check prev_host_hash linkage
            if let Some(ref prev_hash) = curr.prev_host_hash {
                if prev_hash != &prev.bundle_hash {
                    // Emit telemetry event for chain break (100% sampling)
                    if let Some(ref telemetry) = self.telemetry {
                        let identity = IdentityEnvelope::new(
                            "system".to_string(),
                            Domain::Worker,
                            Purpose::Audit,
                            IdentityEnvelope::default_revision(),
                        );
                        let event = TelemetryEventBuilder::new(
                            adapteros_telemetry::EventType::Custom(
                                "federation.chain_break".to_string(),
                            ),
                            LogLevel::Error,
                            format!(
                                "Federation chain break: {} -> {}",
                                prev.host_id, curr.host_id
                            ),
                            identity,
                        )
                        .component("adapteros-federation".to_string())
                        .metadata(json!({
                            "prev_host": prev.host_id,
                            "curr_host": curr.host_id,
                            "expected_prev_hash": prev.bundle_hash,
                            "actual_prev_hash": prev_hash,
                        }))
                        .build();

                        let _ = telemetry.log_event(event);
                    }

                    return Err(AosError::Validation(format!(
                        "Federation chain break: {} -> {} (expected prev_hash: {}, got: {})",
                        prev.host_id, curr.host_id, prev.bundle_hash, prev_hash
                    )));
                }
            } else {
                warn!(
                    host_id = %curr.host_id,
                    "Missing prev_host_hash in chain"
                );
            }

            // Verify timestamp ordering
            if curr.created_at < prev.created_at {
                return Err(AosError::Validation(format!(
                    "Federation chain timestamp violation: {} ({}) -> {} ({})",
                    prev.host_id, prev.created_at, curr.host_id, curr.created_at
                )));
            }
        }

        // Emit telemetry event (100% sampling per Telemetry Ruleset #9)
        if let Some(ref telemetry) = self.telemetry {
            let identity = IdentityEnvelope::new(
                "system".to_string(),
                Domain::Worker,
                Purpose::Audit,
                IdentityEnvelope::default_revision(),
            );
            let event = TelemetryEventBuilder::new(
                adapteros_telemetry::EventType::Custom("federation.chain_verified".to_string()),
                LogLevel::Info,
                format!("Federation chain verified: {} signatures", host_chain.len()),
                identity,
            )
            .component("adapteros-federation".to_string())
            .metadata(json!({
                "chain_length": host_chain.len(),
                "first_host": host_chain.first().map(|s| &s.host_id),
                "last_host": host_chain.last().map(|s| &s.host_id),
                "hosts": host_chain.iter().map(|s| &s.host_id).collect::<Vec<_>>(),
            }))
            .build();

            let _ = telemetry.log_event(event);
        }

        info!(
            chain_length = host_chain.len(),
            first_host = %host_chain.first().unwrap().host_id,
            last_host = %host_chain.last().unwrap().host_id,
            "Federation chain verified"
        );

        Ok(())
    }

    /// Get all signatures for a bundle hash
    pub async fn get_signatures_for_bundle(
        &self,
        bundle_hash: &str,
    ) -> Result<Vec<FederationSignature>> {
        let pool = self.db.pool();

        let rows = sqlx::query(
            r#"
            SELECT id, host_id, bundle_hash, signature, prev_host_hash, created_at, verified
            FROM federation_bundle_signatures
            WHERE bundle_hash = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(bundle_hash)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch signatures: {}", e)))?;

        let signatures = rows
            .into_iter()
            .map(|row| FederationSignature {
                id: Some(row.get::<String, _>("id")),
                host_id: row.get::<String, _>("host_id"),
                bundle_hash: row.get::<String, _>("bundle_hash"),
                signature: row.get::<String, _>("signature"),
                prev_host_hash: row.get::<Option<String>, _>("prev_host_hash"),
                created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                verified: row.get::<i64, _>("verified") != 0,
            })
            .collect();

        Ok(signatures)
    }

    /// Get federation chain for a host
    pub async fn get_host_chain(
        &self,
        host_id: &str,
        limit: usize,
    ) -> Result<Vec<FederationSignature>> {
        let pool = self.db.pool();
        let limit = limit as i64;

        let rows = sqlx::query(
            r#"
            SELECT id, host_id, bundle_hash, signature, prev_host_hash, created_at, verified
            FROM federation_bundle_signatures
            WHERE host_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(host_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch host chain: {}", e)))?;

        let signatures = rows
            .into_iter()
            .map(|row| FederationSignature {
                id: Some(row.get::<String, _>("id")),
                host_id: row.get::<String, _>("host_id"),
                bundle_hash: row.get::<String, _>("bundle_hash"),
                signature: row.get::<String, _>("signature"),
                prev_host_hash: row.get::<Option<String>, _>("prev_host_hash"),
                created_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("created_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                verified: row.get::<i64, _>("verified") != 0,
            })
            .collect();

        Ok(signatures)
    }

    /// Mark a signature as verified
    pub async fn mark_verified(&self, signature_id: &str) -> Result<()> {
        let pool = self.db.pool();

        sqlx::query(
            r#"
            UPDATE federation_bundle_signatures
            SET verified = 1
            WHERE id = ?
            "#,
        )
        .bind(signature_id)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to mark signature as verified: {}", e)))?;

        Ok(())
    }

    /// Serialize bundle metadata for signing
    fn serialize_bundle_metadata(&self, metadata: &StoredBundleMetadata) -> Result<Vec<u8>> {
        serde_json::to_vec(metadata).map_err(AosError::Serialization)
    }

    /// Store signature in database
    async fn store_signature(&self, signature: &FederationSignature) -> Result<()> {
        let pool = self.db.pool();
        let created_at = signature.created_at.to_rfc3339();
        let verified = if signature.verified { 1 } else { 0 };

        sqlx::query(
            r#"
            INSERT INTO federation_bundle_signatures (id, host_id, bundle_hash, signature, prev_host_hash, created_at, verified)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&signature.id)
        .bind(&signature.host_id)
        .bind(&signature.bundle_hash)
        .bind(&signature.signature)
        .bind(&signature.prev_host_hash)
        .bind(created_at)
        .bind(verified)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store signature: {}", e)))?;

        // Link to tick ledger if we have the latest tick hash
        if let Some(tick_hash) = self.get_latest_tick_hash() {
            let _ = self
                .link_to_tick_ledger(&signature.bundle_hash, &tick_hash)
                .await;
        }

        Ok(())
    }

    /// Link federation signature to tick ledger
    async fn link_to_tick_ledger(&self, bundle_hash: &str, tick_hash: &str) -> Result<()> {
        let pool = self.db.pool();

        sqlx::query(
            r#"
            UPDATE tick_ledger
            SET bundle_hash = ?, federation_signature = (
                SELECT signature FROM federation_bundle_signatures WHERE bundle_hash = ? LIMIT 1
            )
            WHERE tick_hash = ?
            "#,
        )
        .bind(bundle_hash)
        .bind(bundle_hash)
        .bind(tick_hash)
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to link to tick ledger: {}", e)))?;

        Ok(())
    }
}

// Pipe trait for ergonomic functional chaining
trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}

// Re-export key types
pub use attestation::{attest_bundle, verify_hardware_attestation, AttestationInfo};
pub use output_hash::{ComparisonResult, OutputHashManager, OutputHashRecord};
pub use peer::{AttestationMetadata, PeerInfo, PeerRegistry};
pub use signature::{BundleSignatureExchange, QuorumManager, QuorumStatus, VerificationResult};

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use tempfile::TempDir;

    async fn setup_test_db() -> Result<Db> {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir
            .path()
            .join(format!("test_{}.db", std::process::id()));
        let db = Db::connect(db_path.to_str().unwrap()).await?;
        db.migrate().await?;
        Ok(db)
    }

    #[tokio::test]
    async fn test_sign_bundle() -> Result<()> {
        let db = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

        let metadata = StoredBundleMetadata {
            bundle_hash: B3Hash::hash(b"test"),
            cpid: Some("cpid-001".to_string()),
            tenant_id: "tenant-001".to_string(),
            event_count: 100,
            sequence_no: 1,
            merkle_root: B3Hash::hash(b"merkle"),
            signature: "test_sig".to_string(),
            created_at: std::time::SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
        };

        let sig = manager.sign_bundle(&metadata).await?;
        assert_eq!(sig.host_id, "test-host");
        assert!(!sig.signature.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_verify_chain() -> Result<()> {
        let db = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

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

        let chain = vec![sig1, sig2];
        manager.verify_cross_host_chain(&chain).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_chain_break_detection() -> Result<()> {
        let db = setup_test_db().await?;
        let keypair = Keypair::generate();
        let manager = FederationManager::with_host_id(db, keypair, "test-host".to_string())?;

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
            Some("wrong_hash".to_string()),
        );

        let chain = vec![sig1, sig2];
        let result = manager.verify_cross_host_chain(&chain).await;
        assert!(result.is_err());

        Ok(())
    }
}
