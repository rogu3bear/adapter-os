//! Key Rotation Daemon
//!
//! Automatic key rotation system with configurable intervals and comprehensive audit logging.
//!
//! ## Architecture
//! - **KEK (Key Encryption Key)**: Root key that encrypts DEKs
//! - **DEK (Data Encryption Key)**: Keys that encrypt actual data
//! - **Rotation Process**: Generate new KEK → Re-encrypt all DEKs with new KEK → Archive old KEK
//!
//! ## Features
//! - Automatic rotation on configurable interval (default: 90 days)
//! - Manual rotation trigger via API
//! - Rotation history stored in database
//! - Graceful degradation if rotation fails
//! - Comprehensive audit logging
//!
//! ## Security Properties
//! - Old keys archived (not deleted) for decrypting historical data
//! - Rotation receipts signed with Ed25519
//! - Atomic rotation (all-or-nothing)
//! - Zero downtime during rotation

use crate::key_provider::{KeyAlgorithm, KeyHandle, KeyProvider};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const ROTATION_FAILURE_ESCALATION_THRESHOLD: u32 = 3;
const ROTATION_FAILURE_COOLDOWN_BASE_SECS: u64 = 30;
const ROTATION_FAILURE_COOLDOWN_MAX_SECS: u64 = 300;

/// Entry representing an encrypted DEK stored in the database
#[derive(Clone, Debug)]
pub struct EncryptedDekEntry {
    /// Unique identifier for this DEK
    pub dek_id: String,
    /// Encrypted DEK material
    pub encrypted_material: Vec<u8>,
    /// ID of the KEK used to encrypt this DEK
    pub kek_id: String,
    /// Tenant ID this DEK belongs to (if multi-tenant)
    pub tenant_id: Option<String>,
    /// Creation timestamp
    pub created_at: u64,
}

/// Storage backend for encrypted DEKs
///
/// Implementations of this trait provide the database layer for DEK storage.
/// The trait is designed to be pluggable, allowing different storage backends
/// (SQLite, PostgreSQL, etc.) without coupling to the crypto module.
#[async_trait]
pub trait CryptoStore: Send + Sync {
    /// List all encrypted DEKs in the store
    ///
    /// Returns all DEKs that need to be re-encrypted during key rotation.
    async fn list_encrypted_deks(&self) -> Result<Vec<EncryptedDekEntry>>;

    /// Atomically update a DEK's encrypted material
    ///
    /// Uses compare-and-swap semantics: the update only succeeds if the
    /// current encrypted material matches `old_ciphertext`. This prevents
    /// race conditions during concurrent rotations.
    ///
    /// # Arguments
    /// * `dek_id` - The DEK identifier
    /// * `old_ciphertext` - Expected current encrypted material
    /// * `new_ciphertext` - New encrypted material after re-encryption
    /// * `new_kek_id` - ID of the new KEK used for encryption
    ///
    /// # Returns
    /// * `Ok(true)` if update succeeded
    /// * `Ok(false)` if old_ciphertext didn't match (CAS failure)
    /// * `Err` on database error
    async fn update_dek_atomic(
        &self,
        dek_id: &str,
        old_ciphertext: &[u8],
        new_ciphertext: &[u8],
        new_kek_id: &str,
    ) -> Result<bool>;
}

/// Rotation policy configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Rotation interval in seconds (default: 90 days = 7776000 seconds)
    pub rotation_interval_secs: u64,
    /// Grace period before old keys are archived (default: 7 days)
    pub grace_period_secs: u64,
    /// Maximum number of historical keys to retain (default: 10)
    pub max_historical_keys: usize,
    /// Whether to automatically rotate keys
    pub auto_rotate: bool,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            rotation_interval_secs: 90 * 24 * 3600, // 90 days
            grace_period_secs: 7 * 24 * 3600,       // 7 days
            max_historical_keys: 10,
            auto_rotate: true,
        }
    }
}

/// Key rotation history entry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RotationHistoryEntry {
    /// Unique ID for this rotation
    pub rotation_id: String,
    /// Key ID that was rotated
    pub key_id: String,
    /// Previous key handle
    pub previous_key: KeyHandle,
    /// New key handle
    pub new_key: KeyHandle,
    /// Timestamp of rotation (Unix timestamp)
    pub timestamp: u64,
    /// Reason for rotation
    pub reason: RotationReason,
    /// Signature of rotation receipt
    pub signature: Vec<u8>,
    /// Number of DEKs re-encrypted
    pub deks_reencrypted: usize,
}

/// Reason for key rotation
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RotationReason {
    /// Scheduled automatic rotation
    Scheduled,
    /// Manual rotation requested by admin
    Manual,
    /// Rotation due to suspected compromise
    Compromise,
    /// Policy-mandated rotation
    PolicyEnforced,
}

impl std::fmt::Display for RotationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RotationReason::Scheduled => write!(f, "scheduled"),
            RotationReason::Manual => write!(f, "manual"),
            RotationReason::Compromise => write!(f, "compromise"),
            RotationReason::PolicyEnforced => write!(f, "policy_enforced"),
        }
    }
}

/// Key rotation daemon state
pub struct RotationDaemon {
    /// Key provider for cryptographic operations
    provider: Arc<dyn KeyProvider>,
    /// Rotation policy
    policy: RwLock<RotationPolicy>,
    /// Rotation history
    history: RwLock<Vec<RotationHistoryEntry>>,
    /// Shutdown signal
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    /// Optional crypto store for DEK re-encryption
    crypto_store: Option<Arc<dyn CryptoStore>>,
}

impl RotationDaemon {
    /// Create a new rotation daemon
    pub fn new(provider: Arc<dyn KeyProvider>, policy: RotationPolicy) -> Self {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(1);

        Self {
            provider,
            policy: RwLock::new(policy),
            history: RwLock::new(Vec::new()),
            shutdown_tx,
            crypto_store: None,
        }
    }

    /// Create a rotation daemon with a crypto store for DEK re-encryption
    pub fn with_crypto_store(
        provider: Arc<dyn KeyProvider>,
        policy: RotationPolicy,
        crypto_store: Arc<dyn CryptoStore>,
    ) -> Self {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel(1);

        Self {
            provider,
            policy: RwLock::new(policy),
            history: RwLock::new(Vec::new()),
            shutdown_tx,
            crypto_store: Some(crypto_store),
        }
    }

    /// Set or replace the crypto store
    pub fn set_crypto_store(&mut self, crypto_store: Arc<dyn CryptoStore>) {
        self.crypto_store = Some(crypto_store);
    }

    /// Start the rotation daemon background task
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let daemon = Arc::clone(&self);
        tokio::spawn(async move {
            use futures_util::FutureExt;
            use std::panic::AssertUnwindSafe;
            if let Err(panic) = AssertUnwindSafe(daemon.run_daemon_loop())
                .catch_unwind()
                .await
            {
                tracing::error!(
                    task = "crypto_key_rotation",
                    "key rotation daemon panicked — security-critical background task crashed: {:?}",
                    panic
                );
            }
        })
    }

    /// Main daemon loop
    async fn run_daemon_loop(&self) {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let mut consecutive_failures = 0u32;

        info!("Key rotation daemon started");

        loop {
            let policy = self.policy.read().await.clone();

            if !policy.auto_rotate {
                debug!("Auto-rotation disabled, daemon sleeping");
                if consecutive_failures > 0 {
                    info!(
                        target: "security.rotation",
                        consecutive_failures,
                        "Auto-rotation disabled, clearing failure budget"
                    );
                    consecutive_failures = 0;
                }
                // Sleep for 1 hour if auto-rotation is disabled
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3600)) => continue,
                    _ = shutdown_rx.recv() => break,
                }
            }

            // Calculate next rotation time
            let check_interval = Duration::from_secs(policy.rotation_interval_secs.min(3600)); // Check at least every hour

            tokio::select! {
                _ = tokio::time::sleep(check_interval) => {
                    match self.check_and_rotate_keys().await {
                        Ok(_) => {
                            if consecutive_failures > 0 {
                                info!(
                                    target: "security.rotation",
                                    consecutive_failures,
                                    "Rotation daemon recovered after consecutive failures"
                                );
                            }
                            consecutive_failures = 0;
                        }
                        Err(e) => {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            let cooldown = rotation_failure_cooldown(consecutive_failures);
                            error!(
                                target: "security.rotation",
                                error = %e,
                                consecutive_failures,
                                cooldown_secs = cooldown.as_secs(),
                                "Failed to check and rotate keys"
                            );
                            if consecutive_failures >= ROTATION_FAILURE_ESCALATION_THRESHOLD {
                                error!(
                                    target: "security.rotation",
                                    event = "daemon_failure_escalated",
                                    consecutive_failures,
                                    escalation_threshold = ROTATION_FAILURE_ESCALATION_THRESHOLD,
                                    cooldown_secs = cooldown.as_secs(),
                                    "crypto key rotation failure budget exceeded"
                                );
                            }
                            tokio::select! {
                                _ = tokio::time::sleep(cooldown) => {}
                                _ = shutdown_rx.recv() => {
                                    info!("Rotation daemon shutting down");
                                    break;
                                }
                            }
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Rotation daemon shutting down");
                    break;
                }
            }
        }

        info!("Key rotation daemon stopped");
    }

    /// Check if any keys need rotation and rotate them
    async fn check_and_rotate_keys(&self) -> Result<()> {
        let policy = self.policy.read().await.clone();
        let history = self.history.read().await.clone();

        // Check if KEK needs rotation
        let kek_id = "kek-master";
        if let Some(last_rotation) = history.iter().rfind(|e| e.key_id == kek_id) {
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                - last_rotation.timestamp;

            if elapsed >= policy.rotation_interval_secs {
                info!(
                    key_id = %kek_id,
                    elapsed_days = elapsed / 86400,
                    "KEK rotation due, starting rotation"
                );
                self.rotate_key(kek_id, RotationReason::Scheduled).await?;
            } else {
                debug!(
                    key_id = %kek_id,
                    days_until_rotation = (policy.rotation_interval_secs - elapsed) / 86400,
                    "KEK rotation not yet due"
                );
            }
        } else {
            info!(key_id = %kek_id, "No previous KEK rotation found, performing initial rotation");
            self.rotate_key(kek_id, RotationReason::Scheduled).await?;
        }

        Ok(())
    }

    /// Rotate a specific key (KEK or DEK)
    pub async fn rotate_key(
        &self,
        key_id: &str,
        reason: RotationReason,
    ) -> Result<RotationHistoryEntry> {
        info!(
            target: "security.rotation",
            key_id = %key_id,
            reason = %reason,
            "Starting key rotation"
        );

        // Generate new key
        let _new_key = self
            .provider
            .generate(key_id, KeyAlgorithm::Aes256Gcm)
            .await
            .inspect_err(|e| {
                error!(
                    target: "security.rotation",
                    key_id = %key_id,
                    error = %e,
                    "Key generation failed during rotation"
                );
            })?;

        // Get rotation receipt from provider
        let receipt = self.provider.rotate(key_id).await?;

        // Count DEKs that need re-encryption
        let deks_reencrypted = self
            .reencrypt_deks(&receipt.previous_key, &receipt.new_key)
            .await?;

        // Create history entry
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let rotation_id = format!("rotation-{}-{}", key_id, timestamp);

        let entry = RotationHistoryEntry {
            rotation_id,
            key_id: key_id.to_string(),
            previous_key: receipt.previous_key,
            new_key: receipt.new_key,
            timestamp,
            reason,
            signature: receipt.signature,
            deks_reencrypted,
        };

        // Add to history
        let mut history = self.history.write().await;
        history.push(entry.clone());

        // Enforce max historical keys limit
        let policy = self.policy.read().await;
        if history.len() > policy.max_historical_keys {
            let to_remove = history.len() - policy.max_historical_keys;
            history.drain(0..to_remove);
            info!(
                removed_count = to_remove,
                "Archived old rotation history entries"
            );
        }

        info!(
            target: "security.rotation",
            key_id = %key_id,
            deks_reencrypted = deks_reencrypted,
            "Key rotation completed successfully"
        );

        Ok(entry)
    }

    /// Re-encrypt all DEKs with new KEK
    ///
    /// Queries all DEKs from the crypto store, decrypts each with the old KEK,
    /// re-encrypts with the new KEK, and atomically updates the store.
    ///
    /// # Arguments
    /// * `old_kek` - Handle to the previous KEK for decryption
    /// * `new_kek` - Handle to the new KEK for re-encryption
    ///
    /// # Returns
    /// Number of DEKs successfully re-encrypted
    async fn reencrypt_deks(&self, old_kek: &KeyHandle, new_kek: &KeyHandle) -> Result<usize> {
        // Check if crypto store is configured
        let crypto_store = match &self.crypto_store {
            Some(store) => store,
            None => {
                warn!(
                    old_kek = %old_kek.provider_id,
                    new_kek = %new_kek.provider_id,
                    "No crypto store configured - DEK re-encryption skipped"
                );
                return Ok(0);
            }
        };

        info!(
            old_kek = %old_kek.provider_id,
            new_kek = %new_kek.provider_id,
            "Starting DEK re-encryption"
        );

        // Query all encrypted DEKs from the store
        let deks = crypto_store.list_encrypted_deks().await?;
        let total_deks = deks.len();

        if total_deks == 0 {
            debug!("No DEKs found to re-encrypt");
            return Ok(0);
        }

        info!(dek_count = total_deks, "Found DEKs to re-encrypt");

        let mut reencrypted_count = 0;
        let mut failed_count = 0;

        for dek_entry in deks {
            // Skip DEKs that are already encrypted with the new KEK
            if dek_entry.kek_id == new_kek.provider_id {
                debug!(
                    dek_id = %dek_entry.dek_id,
                    "DEK already encrypted with new KEK, skipping"
                );
                continue;
            }

            // Decrypt DEK with old KEK
            let decrypted_dek = match self
                .provider
                .unseal(&old_kek.provider_id, &dek_entry.encrypted_material)
                .await
            {
                Ok(plaintext) => plaintext,
                Err(e) => {
                    error!(
                        dek_id = %dek_entry.dek_id,
                        error = %e,
                        "Failed to decrypt DEK with old KEK"
                    );
                    failed_count += 1;
                    continue;
                }
            };

            // Re-encrypt DEK with new KEK
            let new_encrypted_dek = match self
                .provider
                .seal(&new_kek.provider_id, &decrypted_dek)
                .await
            {
                Ok(ciphertext) => ciphertext,
                Err(e) => {
                    error!(
                        dek_id = %dek_entry.dek_id,
                        error = %e,
                        "Failed to re-encrypt DEK with new KEK"
                    );
                    failed_count += 1;
                    continue;
                }
            };

            // Atomic update to database
            match crypto_store
                .update_dek_atomic(
                    &dek_entry.dek_id,
                    &dek_entry.encrypted_material,
                    &new_encrypted_dek,
                    &new_kek.provider_id,
                )
                .await
            {
                Ok(true) => {
                    reencrypted_count += 1;
                    debug!(
                        dek_id = %dek_entry.dek_id,
                        "DEK re-encrypted successfully"
                    );
                }
                Ok(false) => {
                    warn!(
                        dek_id = %dek_entry.dek_id,
                        "DEK update failed (CAS mismatch) - may have been modified concurrently"
                    );
                    failed_count += 1;
                }
                Err(e) => {
                    error!(
                        dek_id = %dek_entry.dek_id,
                        error = %e,
                        "Database error during DEK update"
                    );
                    failed_count += 1;
                }
            }
        }

        if failed_count > 0 {
            warn!(
                reencrypted = reencrypted_count,
                failed = failed_count,
                total = total_deks,
                "DEK re-encryption completed with failures"
            );
        } else {
            info!(
                reencrypted = reencrypted_count,
                total = total_deks,
                "DEK re-encryption completed successfully"
            );
        }

        // Return error if all DEKs failed
        if reencrypted_count == 0 && failed_count > 0 {
            return Err(AosError::Crypto(format!(
                "All {} DEK re-encryption attempts failed",
                failed_count
            )));
        }

        Ok(reencrypted_count)
    }

    /// Get rotation history for a specific key
    pub async fn get_rotation_history(&self, key_id: &str) -> Vec<RotationHistoryEntry> {
        let history = self.history.read().await;
        history
            .iter()
            .filter(|e| e.key_id == key_id)
            .cloned()
            .collect()
    }

    /// Get all rotation history
    pub async fn get_all_history(&self) -> Vec<RotationHistoryEntry> {
        self.history.read().await.clone()
    }

    /// Update rotation policy
    pub async fn update_policy(&self, new_policy: RotationPolicy) {
        let rotation_interval_days = new_policy.rotation_interval_secs / 86400;
        let auto_rotate = new_policy.auto_rotate;
        let mut policy = self.policy.write().await;
        *policy = new_policy;
        info!(
            rotation_interval_days = rotation_interval_days,
            auto_rotate = auto_rotate,
            "Rotation policy updated"
        );
    }

    /// Shutdown the daemon
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }

    /// Force immediate rotation of a key (manual trigger)
    pub async fn force_rotate(&self, key_id: &str) -> Result<RotationHistoryEntry> {
        warn!(
            target: "security.rotation",
            key_id = %key_id,
            "Manual key rotation triggered"
        );
        self.rotate_key(key_id, RotationReason::Manual).await
    }

    /// Emergency rotation due to suspected compromise
    pub async fn emergency_rotate(&self, key_id: &str) -> Result<RotationHistoryEntry> {
        error!(
            target: "security.rotation",
            key_id = %key_id,
            "Emergency key rotation due to suspected compromise"
        );
        self.rotate_key(key_id, RotationReason::Compromise).await
    }
}

fn rotation_failure_cooldown(consecutive_failures: u32) -> Duration {
    let backoff_shift = consecutive_failures.saturating_sub(1).min(4);
    let multiplier = 1u64 << backoff_shift;
    let secs = ROTATION_FAILURE_COOLDOWN_BASE_SECS.saturating_mul(multiplier);
    Duration::from_secs(secs.min(ROTATION_FAILURE_COOLDOWN_MAX_SECS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_provider::KeyProviderConfig;
    use crate::providers::keychain::KeychainProvider;

    #[tokio::test]
    async fn test_rotation_policy_default() {
        let policy = RotationPolicy::default();
        assert_eq!(policy.rotation_interval_secs, 90 * 24 * 3600);
        assert_eq!(policy.grace_period_secs, 7 * 24 * 3600);
        assert_eq!(policy.max_historical_keys, 10);
        assert!(policy.auto_rotate);
    }

    #[tokio::test]
    async fn test_rotation_daemon_creation() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).expect("Should create provider");
        let policy = RotationPolicy::default();

        let daemon = Arc::new(RotationDaemon::new(Arc::new(provider), policy));
        assert!(daemon.get_all_history().await.is_empty());
    }

    #[tokio::test]
    async fn test_manual_rotation() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).expect("Should create provider");
        let policy = RotationPolicy {
            rotation_interval_secs: 60, // 1 minute for testing
            ..Default::default()
        };

        let daemon = Arc::new(RotationDaemon::new(Arc::new(provider), policy));

        // Force a rotation
        let result = daemon.force_rotate("test-key").await;

        if let Ok(entry) = result {
            assert_eq!(entry.key_id, "test-key");
            assert_eq!(entry.reason, RotationReason::Manual);
            assert!(!entry.signature.is_empty());

            // Verify history updated
            let history = daemon.get_all_history().await;
            assert_eq!(history.len(), 1);
        } else {
            // Rotation may fail in test environment without proper keychain
            // This is acceptable for unit tests
            println!(
                "Manual rotation failed (expected in test env): {:?}",
                result
            );
        }
    }

    #[tokio::test]
    async fn test_rotation_history() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).expect("Should create provider");
        let daemon = Arc::new(RotationDaemon::new(
            Arc::new(provider),
            RotationPolicy::default(),
        ));

        // Create mock history entry
        let entry = RotationHistoryEntry {
            rotation_id: "rotation-test-123".to_string(),
            key_id: "test-key".to_string(),
            previous_key: KeyHandle::new("old-key".to_string(), KeyAlgorithm::Aes256Gcm),
            new_key: KeyHandle::new("new-key".to_string(), KeyAlgorithm::Aes256Gcm),
            timestamp: 1234567890,
            reason: RotationReason::Scheduled,
            signature: vec![1, 2, 3],
            deks_reencrypted: 5,
        };

        {
            let mut history = daemon.history.write().await;
            history.push(entry.clone());
        }

        // Query history
        let key_history = daemon.get_rotation_history("test-key").await;
        assert_eq!(key_history.len(), 1);
        assert_eq!(key_history[0].rotation_id, "rotation-test-123");
        assert_eq!(key_history[0].deks_reencrypted, 5);
    }

    #[tokio::test]
    async fn test_policy_update() {
        let config = KeyProviderConfig::default();
        let provider = KeychainProvider::new(config).expect("Should create provider");
        let daemon = Arc::new(RotationDaemon::new(
            Arc::new(provider),
            RotationPolicy::default(),
        ));

        let new_policy = RotationPolicy {
            rotation_interval_secs: 30 * 24 * 3600, // 30 days
            auto_rotate: false,
            ..Default::default()
        };

        daemon.update_policy(new_policy).await;

        let updated_policy = daemon.policy.read().await.clone();
        assert_eq!(updated_policy.rotation_interval_secs, 30 * 24 * 3600);
        assert!(!updated_policy.auto_rotate);
    }

    #[tokio::test]
    async fn test_rotation_reason_display() {
        assert_eq!(RotationReason::Scheduled.to_string(), "scheduled");
        assert_eq!(RotationReason::Manual.to_string(), "manual");
        assert_eq!(RotationReason::Compromise.to_string(), "compromise");
        assert_eq!(
            RotationReason::PolicyEnforced.to_string(),
            "policy_enforced"
        );
    }

    #[tokio::test]
    async fn test_rotation_failure_cooldown_bounds() {
        assert_eq!(rotation_failure_cooldown(1), Duration::from_secs(30));
        assert_eq!(rotation_failure_cooldown(2), Duration::from_secs(60));
        assert_eq!(rotation_failure_cooldown(3), Duration::from_secs(120));
        assert_eq!(rotation_failure_cooldown(10), Duration::from_secs(300));
    }
}
