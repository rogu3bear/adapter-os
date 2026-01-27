//! Key rotation daemon with signed audit receipts
//!
//! This daemon runs continuously and rotates cryptographic keys at configured intervals,
//! writing signed rotation receipts to var/audit/keys/ for crash-safe audit trails.
//!
//! ## Stub Implementation: KMS Provider Mode
//!
//! The [`RotationDaemon::new`] constructor contains a stub for KMS provider integration.
//! When `KeyProviderMode::Kms` is configured, the daemon returns an error because
//! external KMS providers are not yet implemented.
//!
//! ### Why This Is a Stub
//!
//! Production deployments often require integration with external Key Management Systems
//! for centralized key management, compliance, and audit requirements. Common KMS
//! providers include:
//!
//! - **AWS KMS**: Hardware Security Module (HSM)-backed keys in AWS
//! - **HashiCorp Vault**: Secret management with multiple backends
//! - **Azure Key Vault**: Microsoft's cloud-based key management
//! - **Google Cloud KMS**: Google's managed key service
//!
//! Each provider requires:
//! - Provider-specific SDK dependencies
//! - Authentication configuration (IAM roles, service accounts, tokens)
//! - Network connectivity (not suitable for air-gapped deployments)
//!
//! ### What Would Be Needed for Full Implementation
//!
//! 1. **Provider SDK integration**:
//!    ```rust,ignore
//!    // Example for AWS KMS
//!    use aws_sdk_kms::Client as KmsClient;
//!    ```
//!
//! 2. **KmsProvider trait implementation** in `adapteros-crypto`:
//!    - `rotate()` - Generate new key version in KMS
//!    - `sign()` - Sign data using KMS key
//!    - `verify()` - Verify signatures
//!
//! 3. **Configuration** for provider selection and credentials:
//!    ```toml
//!    [key_provider]
//!    mode = "kms"
//!    kms_provider = "aws"  # or "vault", "azure", "gcp"
//!    kms_key_id = "arn:aws:kms:..."
//!    ```
//!
//! ### Current Stub Behavior
//!
//! When `KeyProviderMode::Kms` is selected:
//! - Returns `Err(AosError::Crypto("KMS provider not yet implemented..."))`
//! - Logs indicate the unsupported configuration
//! - The daemon does not start
//!
//! ### Supported Modes
//!
//! Currently only `KeyProviderMode::Keychain` is fully supported, which uses the
//! macOS Keychain for key storage with optional Secure Enclave backing.

use adapteros_core::{AosError, Result};
use adapteros_crypto::{KeyProvider, KeyProviderConfig, KeychainProvider, RotationReceipt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;
use tracing::{error, info, warn};

/// Key rotation daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationDaemonConfig {
    /// Key provider configuration
    pub key_provider: KeyProviderConfig,
    /// Rotation interval in seconds
    pub rotation_interval_secs: u64,
    /// Keys to rotate (by ID)
    pub keys_to_rotate: Vec<String>,
    /// Audit log directory path
    pub audit_log_path: PathBuf,
    /// Maximum number of receipts to keep per key
    pub max_receipts_per_key: usize,
}

impl Default for RotationDaemonConfig {
    fn default() -> Self {
        Self {
            key_provider: KeyProviderConfig::default(),
            rotation_interval_secs: 86400, // 24 hours
            keys_to_rotate: vec![
                "tenant-signing-key".to_string(),
                "tenant-encryption-key".to_string(),
                "system-audit-key".to_string(),
            ],
            audit_log_path: PathBuf::from("var/audit/keys"),
            max_receipts_per_key: 100,
        }
    }
}

/// Key rotation daemon
pub struct RotationDaemon {
    config: RotationDaemonConfig,
    key_provider: Arc<dyn KeyProvider + Send + Sync>,
    audit_state: Arc<Mutex<AuditState>>,
}

impl RotationDaemon {
    /// Create a new rotation daemon with the specified configuration.
    ///
    /// # Key Provider Selection
    ///
    /// The daemon supports multiple key provider backends:
    ///
    /// | Mode | Status | Description |
    /// |------|--------|-------------|
    /// | `Keychain` | **Supported** | macOS Keychain with optional Secure Enclave |
    /// | `Kms` | **Stub** | External KMS integration (not yet implemented) |
    /// | `File` | **Blocked** | File-based keys disallowed in production daemon |
    ///
    /// ## KMS Mode (Stub)
    ///
    /// The `Kms` mode is a stub that returns an error. Full implementation would
    /// require integration with external KMS providers (AWS KMS, HashiCorp Vault,
    /// Azure Key Vault, etc.) which need provider-specific SDKs and authentication.
    ///
    /// See the [module-level documentation](self) for details on what would be
    /// needed to implement KMS support.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `KeyProviderMode::Kms` is configured (stub - not implemented)
    /// - `KeyProviderMode::File` is configured (blocked for security)
    /// - The Keychain provider fails to initialize
    pub async fn new(config: RotationDaemonConfig) -> Result<Self> {
        // Initialize key provider based on configured mode
        let key_provider: Arc<dyn KeyProvider + Send + Sync> = match config.key_provider.mode {
            adapteros_crypto::KeyProviderMode::Keychain => {
                Arc::new(KeychainProvider::new(config.key_provider.clone())?)
            }
            adapteros_crypto::KeyProviderMode::Kms => {
                // STUB: KMS provider integration is not yet implemented.
                // Full implementation would require:
                // - Provider-specific SDK (aws-sdk-kms, hashicorp-vault, etc.)
                // - Authentication configuration
                // - Network connectivity (incompatible with air-gapped deployments)
                return Err(AosError::Crypto(
                    "KMS provider not yet implemented for rotation daemon".to_string(),
                ));
            }
            adapteros_crypto::KeyProviderMode::File => {
                // File-based key storage is explicitly disallowed in the production
                // rotation daemon for security reasons. Use Keychain or KMS instead.
                return Err(AosError::Crypto(
                    "File provider not allowed in production daemon".to_string(),
                ));
            }
        };

        // Load existing audit state
        let audit_state = AuditState::load(&config.audit_log_path).await?;

        Ok(Self {
            config,
            key_provider,
            audit_state: Arc::new(Mutex::new(audit_state)),
        })
    }

    /// Start the rotation daemon
    pub async fn start(self: Arc<Self>) -> Result<()> {
        info!(
            interval_secs = self.config.rotation_interval_secs,
            "Starting key rotation daemon"
        );

        // Ensure audit directory exists
        tokio::fs::create_dir_all(&self.config.audit_log_path)
            .await
            .map_err(|e| {
                AosError::Io(format!(
                    "Failed to create audit directory {}: {}",
                    self.config.audit_log_path.display(),
                    e
                ))
            })?;

        // Spawn the rotation task
        tokio::spawn(async move {
            self.run_rotation_loop().await;
        });

        Ok(())
    }

    /// Main rotation loop
    async fn run_rotation_loop(&self) {
        let mut interval = time::interval(Duration::from_secs(self.config.rotation_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.perform_rotation_cycle().await {
                error!("Key rotation cycle failed: {}", e);
                // Continue running despite errors
            }
        }
    }

    /// Perform one complete rotation cycle for all configured keys
    async fn perform_rotation_cycle(&self) -> Result<()> {
        info!("Starting key rotation cycle");

        let mut rotated_keys = 0;
        let mut errors = 0;

        for key_id in &self.config.keys_to_rotate {
            match self.rotate_single_key(key_id).await {
                Ok(_) => {
                    rotated_keys += 1;
                    info!(key_id = %key_id, "Successfully rotated key");
                }
                Err(e) => {
                    errors += 1;
                    error!(key_id = %key_id, error = %e, "Failed to rotate key");
                }
            }
        }

        info!(
            rotated_keys,
            total_keys = self.config.keys_to_rotate.len(),
            errors,
            "Completed key rotation cycle"
        );

        Ok(())
    }

    /// Rotate a single key and write signed receipt
    async fn rotate_single_key(&self, key_id: &str) -> Result<()> {
        // Perform the rotation
        let receipt = self.key_provider.rotate(key_id).await?;

        // Write the receipt atomically
        self.write_rotation_receipt(&receipt).await?;

        // Update audit state
        let mut audit_state = self.audit_state.lock().await;
        audit_state.add_receipt(key_id.to_string(), receipt.clone());

        // Prune old receipts if needed
        audit_state.prune_old_receipts(key_id, self.config.max_receipts_per_key);

        // Save updated audit state
        audit_state.save(&self.config.audit_log_path).await?;

        Ok(())
    }

    /// Write a rotation receipt to disk atomically
    async fn write_rotation_receipt(&self, receipt: &RotationReceipt) -> Result<()> {
        let timestamp = receipt.timestamp;
        let key_id = &receipt.key_id;

        // Create receipt filename: key_id-timestamp.receipt
        let filename = format!("{}-{}.receipt", key_id, timestamp);
        let receipt_path = self.config.audit_log_path.join(filename);

        // Serialize receipt
        let receipt_json =
            serde_json::to_string_pretty(receipt).map_err(AosError::Serialization)?;

        // Write atomically using temp file + rename
        let temp_path = receipt_path.with_extension("tmp");
        tokio::fs::write(&temp_path, &receipt_json)
            .await
            .map_err(|e| AosError::Io(format!("Failed to write rotation receipt: {}", e)))?;

        tokio::fs::rename(&temp_path, &receipt_path)
            .await
            .map_err(|e| {
                AosError::Io(format!("Failed to atomically move rotation receipt: {}", e))
            })?;

        info!(
            key_id = %receipt.key_id,
            timestamp = receipt.timestamp,
            path = %receipt_path.display(),
            "Wrote signed rotation receipt"
        );

        Ok(())
    }

    /// Manually trigger rotation for a specific key (for on-demand rotation)
    pub async fn rotate_key_now(&self, key_id: &str) -> Result<RotationReceipt> {
        info!(key_id = %key_id, "Manually triggering key rotation");

        // Check if key is configured for rotation
        if !self.config.keys_to_rotate.contains(&key_id.to_string()) {
            warn!(
                key_id = %key_id,
                "Key not configured for rotation, but proceeding anyway"
            );
        }

        let receipt = self.key_provider.rotate(key_id).await?;
        self.write_rotation_receipt(&receipt).await?;

        let mut audit_state = self.audit_state.lock().await;
        audit_state.add_receipt(key_id.to_string(), receipt.clone());
        audit_state.prune_old_receipts(key_id, self.config.max_receipts_per_key);
        audit_state.save(&self.config.audit_log_path).await?;

        Ok(receipt)
    }

    /// Get rotation history for a key
    pub async fn get_rotation_history(&self, key_id: &str) -> Vec<RotationReceipt> {
        let audit_state = self.audit_state.lock().await;
        audit_state.get_receipts(key_id).cloned().collect()
    }
}

/// Audit state tracking rotation receipts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditState {
    /// Receipts by key ID
    receipts: HashMap<String, Vec<RotationReceipt>>,
}

impl AuditState {
    /// Load audit state from disk
    async fn load(audit_path: &Path) -> Result<Self> {
        let state_path = audit_path.join("audit_state.json");

        if state_path.exists() {
            let state_data = tokio::fs::read(&state_path)
                .await
                .map_err(|e| AosError::Io(format!("Failed to read audit state: {}", e)))?;

            serde_json::from_slice(&state_data).map_err(AosError::Serialization)
        } else {
            // Create new empty state
            Ok(Self {
                receipts: HashMap::new(),
            })
        }
    }

    /// Save audit state to disk atomically
    async fn save(&self, audit_path: &Path) -> Result<()> {
        let state_path = audit_path.join("audit_state.json");
        let temp_path = state_path.with_extension("tmp");

        let state_json = serde_json::to_string_pretty(self).map_err(AosError::Serialization)?;

        tokio::fs::write(&temp_path, &state_json)
            .await
            .map_err(|e| AosError::Io(format!("Failed to write audit state: {}", e)))?;

        tokio::fs::rename(&temp_path, &state_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to atomically move audit state: {}", e)))?;

        Ok(())
    }

    /// Add a rotation receipt
    fn add_receipt(&mut self, key_id: String, receipt: RotationReceipt) {
        self.receipts.entry(key_id).or_default().push(receipt);
    }

    /// Get receipts for a key
    fn get_receipts(&self, key_id: &str) -> impl Iterator<Item = &RotationReceipt> {
        self.receipts
            .get(key_id)
            .map(|receipts| receipts.iter())
            .unwrap_or_else(|| [].iter())
    }

    /// Prune old receipts for a key, keeping only the most recent N
    fn prune_old_receipts(&mut self, key_id: &str, max_receipts: usize) {
        if let Some(receipts) = self.receipts.get_mut(key_id) {
            if receipts.len() > max_receipts {
                // Sort by timestamp descending (newest first)
                receipts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                // Keep only the most recent receipts
                receipts.truncate(max_receipts);

                info!(
                    key_id = %key_id,
                    kept = max_receipts,
                    pruned = receipts.len().saturating_sub(max_receipts),
                    "Pruned old rotation receipts"
                );
            }
        }
    }
}

/// Create and start a rotation daemon with default configuration
pub async fn start_rotation_daemon() -> Result<()> {
    let config = RotationDaemonConfig::default();
    let daemon = Arc::new(RotationDaemon::new(config).await?);
    daemon.start().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_crypto::{KeyAlgorithm, KeyHandle};
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-").expect("create temp dir")
    }

    #[tokio::test]
    async fn test_audit_state_persistence() {
        let temp_dir = new_test_tempdir();
        let audit_path = temp_dir.path();

        // Create and save state
        let mut state = AuditState {
            receipts: HashMap::new(),
        };

        let receipt = RotationReceipt::new(
            "test-key".to_string(),
            KeyHandle::new("old-handle".to_string(), KeyAlgorithm::Ed25519),
            KeyHandle::new("new-handle".to_string(), KeyAlgorithm::Ed25519),
            1234567890,
            vec![1, 2, 3, 4],
        );

        state.add_receipt("test-key".to_string(), receipt.clone());
        state.save(audit_path).await.unwrap();

        // Load and verify
        let loaded_state = AuditState::load(audit_path).await.unwrap();
        let receipts: Vec<_> = loaded_state.get_receipts("test-key").collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].key_id, "test-key");
    }

    #[tokio::test]
    async fn test_receipt_pruning() {
        let mut state = AuditState {
            receipts: HashMap::new(),
        };

        // Add multiple receipts
        for i in 0..5 {
            let receipt = RotationReceipt::new(
                "test-key".to_string(),
                KeyHandle::new(format!("old-{}", i), KeyAlgorithm::Ed25519),
                KeyHandle::new(format!("new-{}", i), KeyAlgorithm::Ed25519),
                1234567890 + i,
                vec![i as u8; 4],
            );
            state.add_receipt("test-key".to_string(), receipt);
        }

        // Prune to keep only 2
        state.prune_old_receipts("test-key", 2);

        let receipts: Vec<_> = state.get_receipts("test-key").collect();
        assert_eq!(receipts.len(), 2);

        // Verify we kept the newest (highest timestamps)
        assert_eq!(receipts[0].timestamp, 1234567890 + 4);
        assert_eq!(receipts[1].timestamp, 1234567890 + 3);
    }
}
