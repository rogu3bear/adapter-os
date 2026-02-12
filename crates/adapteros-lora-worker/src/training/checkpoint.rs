//! Training checkpoint management for resumable training
//!
//! Enables saving and restoring training state, allowing training to resume
//! from interruptions or to implement strategies like best-model-restore.
//!
//! ## Cryptographic Integrity
//!
//! Checkpoints are signed with BLAKE3 + Ed25519 on save and verified on load.
//! A `.sig` sidecar file accompanies each `.ckpt` file. In release builds,
//! unsigned or tampered checkpoints are rejected. In debug builds, a warning
//! is emitted but loading proceeds.
#![allow(clippy::useless_vec)]

use super::trainer::{LoRAWeights, MultiModuleOptimizerState, TrainingConfig};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::{Keypair, PublicKey, Signature};
use adapteros_types::training::TRAINING_DATA_CONTRACT_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Checkpoint signature sidecar
// ---------------------------------------------------------------------------

/// Schema version for checkpoint signatures. Bump when the sidecar format changes.
const CHECKPOINT_SIG_SCHEMA_VERSION: u8 = 1;

/// Sidecar file containing BLAKE3 hash + Ed25519 signature for a checkpoint.
///
/// Written atomically alongside the `.ckpt` file during save.
/// Verified before parsing checkpoint JSON during load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSignature {
    /// Schema version (for forward compatibility)
    pub schema_version: u8,
    /// BLAKE3 hash of the checkpoint JSON bytes
    pub blake3_hash: B3Hash,
    /// Ed25519 signature over the BLAKE3 hash bytes
    pub signature: Signature,
    /// Public key used for signing (embedded for standalone verification)
    pub public_key: PublicKey,
    /// ISO 8601 timestamp when the signature was created
    pub signed_at: String,
}

impl CheckpointSignature {
    /// Sign checkpoint content bytes, producing a sidecar structure.
    pub fn sign(content: &[u8], keypair: &Keypair) -> Self {
        let blake3_hash = B3Hash::hash(content);
        let signature = keypair.sign(blake3_hash.as_bytes());
        let public_key = keypair.public_key();
        Self {
            schema_version: CHECKPOINT_SIG_SCHEMA_VERSION,
            blake3_hash,
            signature,
            public_key,
            signed_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Verify that `content` matches the stored hash and that the signature is valid.
    pub fn verify(&self, content: &[u8]) -> Result<()> {
        if self.schema_version != CHECKPOINT_SIG_SCHEMA_VERSION {
            return Err(AosError::CheckpointIntegrity(format!(
                "Signature schema version mismatch: expected {}, got {}",
                CHECKPOINT_SIG_SCHEMA_VERSION, self.schema_version
            )));
        }

        let actual_hash = B3Hash::hash(content);
        if actual_hash != self.blake3_hash {
            return Err(AosError::CheckpointIntegrity(format!(
                "BLAKE3 hash mismatch: expected {}, got {}",
                self.blake3_hash.to_hex(),
                actual_hash.to_hex()
            )));
        }

        self.public_key
            .verify(self.blake3_hash.as_bytes(), &self.signature)
            .map_err(|e| {
                AosError::CheckpointIntegrity(format!(
                    "Ed25519 signature verification failed: {}",
                    e
                ))
            })
    }

    /// Serialize to JSON bytes.
    fn to_json(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| {
            AosError::CheckpointIntegrity(format!("Failed to serialize signature: {}", e))
        })
    }

    /// Deserialize from JSON bytes.
    fn from_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            AosError::CheckpointIntegrity(format!("Failed to deserialize signature: {}", e))
        })
    }
}

/// Derive the `.sig` sidecar path from a checkpoint path.
fn sig_path_for(ckpt_path: &Path) -> PathBuf {
    if let Some(file_name) = ckpt_path.file_name() {
        let mut sig_name = file_name.to_os_string();
        sig_name.push(".sig");
        let mut sig_path = ckpt_path.to_path_buf();
        sig_path.set_file_name(sig_name);
        sig_path
    } else {
        let mut fallback = ckpt_path.as_os_str().to_owned();
        fallback.push(".sig");
        PathBuf::from(fallback)
    }
}

/// Check whether we are running a release build (used for strict vs. permissive mode).
fn is_release_build() -> bool {
    !cfg!(debug_assertions)
}

// ---------------------------------------------------------------------------
// TrainingCheckpoint
// ---------------------------------------------------------------------------

/// Training checkpoint containing complete state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingCheckpoint {
    /// Epoch number (0-indexed)
    pub epoch: u32,
    /// Current step within epoch
    pub step: u32,
    /// Current loss value
    pub loss: f32,
    /// Learning rate at this checkpoint
    pub learning_rate: f32,
    /// Training data contract version.
    pub training_contract_version: String,
    /// Training configuration
    pub config: TrainingConfig,
    /// LoRA weights at this checkpoint
    pub weights: LoRAWeights,
    /// Best loss seen so far (for early stopping)
    pub best_loss: f32,
    /// Epochs without improvement (for early stopping)
    pub epochs_without_improvement: u32,
    /// Timestamp when checkpoint was created
    pub timestamp: String,
    /// Optional metadata
    pub metadata: std::collections::HashMap<String, String>,
    /// Multi-module optimizer state (for multi-module training resume)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_module_optimizer_state: Option<MultiModuleOptimizerState>,
}

impl TrainingCheckpoint {
    /// Create a new training checkpoint
    pub fn new(
        epoch: u32,
        step: u32,
        loss: f32,
        learning_rate: f32,
        config: TrainingConfig,
        weights: LoRAWeights,
    ) -> Self {
        Self {
            epoch,
            step,
            loss,
            learning_rate,
            training_contract_version: config.training_contract_version.clone(),
            config,
            weights,
            best_loss: loss,
            epochs_without_improvement: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: std::collections::HashMap::new(),
            multi_module_optimizer_state: None,
        }
    }

    /// Create a new training checkpoint with multi-module optimizer state
    pub fn new_with_optimizer_state(
        epoch: u32,
        step: u32,
        loss: f32,
        learning_rate: f32,
        config: TrainingConfig,
        weights: LoRAWeights,
        optimizer_state: MultiModuleOptimizerState,
    ) -> Self {
        Self {
            epoch,
            step,
            loss,
            learning_rate,
            training_contract_version: config.training_contract_version.clone(),
            config,
            weights,
            best_loss: loss,
            epochs_without_improvement: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: std::collections::HashMap::new(),
            multi_module_optimizer_state: Some(optimizer_state),
        }
    }

    /// Save checkpoint to file using atomic write pattern to prevent corruption.
    ///
    /// When a `signing_key` is provided, a `.sig` sidecar is written atomically
    /// alongside the checkpoint file. The sidecar contains a BLAKE3 hash of the
    /// checkpoint JSON and an Ed25519 signature over that hash.
    pub async fn save<P: AsRef<Path>>(&self, path: P, signing_key: Option<&Keypair>) -> Result<()> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Training(format!("Failed to create checkpoint directory: {}", e))
            })?;
        }

        // Serialize to JSON
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AosError::Training(format!("Failed to serialize checkpoint: {}", e)))?;
        let json_bytes = json.as_bytes();

        // Compute signature sidecar if signing key is provided
        let sig_sidecar = signing_key.map(|kp| CheckpointSignature::sign(json_bytes, kp));

        // -- Atomic write: checkpoint -------------------------------------------
        let temp_path = path.with_extension("ckpt.tmp");

        tokio::fs::write(&temp_path, json_bytes)
            .await
            .map_err(|e| {
                AosError::Training(format!("Failed to write checkpoint to temp file: {}", e))
            })?;

        // -- Atomic write: signature sidecar ------------------------------------
        let sig_file = sig_path_for(path);
        let sig_temp = sig_file.with_extension("sig.tmp");

        if let Some(ref sig) = sig_sidecar {
            let sig_json = sig.to_json()?;
            tokio::fs::write(&sig_temp, &sig_json).await.map_err(|e| {
                AosError::CheckpointIntegrity(format!("Failed to write signature temp file: {}", e))
            })?;
        }

        // Rename checkpoint (atomic on POSIX)
        if let Err(e) = tokio::fs::rename(&temp_path, path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            let _ = tokio::fs::remove_file(&sig_temp).await;
            return Err(AosError::Training(format!(
                "Failed to rename checkpoint file: {}",
                e
            )));
        }

        // Rename signature sidecar
        if sig_sidecar.is_some() {
            if let Err(e) = tokio::fs::rename(&sig_temp, &sig_file).await {
                // Checkpoint was already committed — warn but don't fail the save.
                // The next save will overwrite with a correct pair.
                warn!(
                    path = %sig_file.display(),
                    error = %e,
                    "Failed to rename signature sidecar (checkpoint saved without signature)"
                );
            }
        }

        info!(
            path = %path.display(),
            epoch = self.epoch,
            loss = self.loss,
            signed = sig_sidecar.is_some(),
            "Checkpoint saved successfully"
        );

        Ok(())
    }

    /// Load checkpoint from file with integrity verification.
    ///
    /// If a `.sig` sidecar exists, the BLAKE3 hash and Ed25519 signature are
    /// verified before the JSON is parsed. In release builds, a missing or
    /// invalid signature causes a hard error. In debug builds, a warning is
    /// emitted but the checkpoint is still loaded.
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Read checkpoint bytes
        let json_bytes = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read checkpoint: {}", e)))?;

        if json_bytes.is_empty() {
            return Err(AosError::Training(format!(
                "Checkpoint file is empty: {}",
                path.display()
            )));
        }

        // -- Signature verification --------------------------------------------
        let sig_file = sig_path_for(path);
        match tokio::fs::read(&sig_file).await {
            Ok(sig_bytes) => {
                let sig = CheckpointSignature::from_json(&sig_bytes)?;
                if let Err(e) = sig.verify(&json_bytes) {
                    if is_release_build() {
                        return Err(e);
                    }
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Checkpoint signature verification failed (dev mode — proceeding)"
                    );
                } else {
                    debug!(
                        path = %path.display(),
                        blake3 = %sig.blake3_hash.to_hex(),
                        "Checkpoint signature verified"
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if is_release_build() {
                    return Err(AosError::CheckpointIntegrity(format!(
                        "Signature sidecar missing for checkpoint: {}",
                        path.display()
                    )));
                }
                warn!(
                    path = %sig_file.display(),
                    "Checkpoint signature file not found (dev mode — proceeding unsigned)"
                );
            }
            Err(e) => {
                return Err(AosError::CheckpointIntegrity(format!(
                    "Failed to read signature sidecar: {}",
                    e
                )));
            }
        }

        // -- Deserialize -------------------------------------------------------
        let json = String::from_utf8(json_bytes).map_err(|e| {
            AosError::Training(format!("Checkpoint file is not valid UTF-8: {}", e))
        })?;

        let checkpoint: Self = serde_json::from_str(&json).map_err(|e| {
            AosError::Training(format!(
                "Failed to deserialize checkpoint (possible corruption): {} at line {}, column {}",
                e,
                e.line(),
                e.column()
            ))
        })?;

        // Basic sanity checks
        if checkpoint.epoch > 10000 {
            return Err(AosError::Training(format!(
                "Invalid checkpoint: epoch {} exceeds reasonable bounds (possible corruption)",
                checkpoint.epoch
            )));
        }

        if !checkpoint.loss.is_finite() {
            return Err(AosError::Training(format!(
                "Invalid checkpoint: loss {} is not finite (possible corruption)",
                checkpoint.loss
            )));
        }
        if checkpoint.training_contract_version != TRAINING_DATA_CONTRACT_VERSION {
            return Err(AosError::Training(format!(
                "Checkpoint training contract version mismatch: expected {}, got {}",
                TRAINING_DATA_CONTRACT_VERSION, checkpoint.training_contract_version
            )));
        }
        if checkpoint.training_contract_version != checkpoint.config.training_contract_version {
            return Err(AosError::Training(
                "Checkpoint training contract version differs from config".to_string(),
            ));
        }

        info!(
            path = %path.display(),
            epoch = checkpoint.epoch,
            loss = checkpoint.loss,
            "Checkpoint loaded and validated successfully"
        );

        Ok(checkpoint)
    }

    /// Add metadata to checkpoint
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Update best loss tracking
    pub fn update_best_loss(&mut self, best_loss: f32, epochs_without_improvement: u32) {
        self.best_loss = best_loss;
        self.epochs_without_improvement = epochs_without_improvement;
    }
}

// ---------------------------------------------------------------------------
// CheckpointManager
// ---------------------------------------------------------------------------

/// Checkpoint manager for handling multiple checkpoints.
/// Implements Clone to allow spawning background checkpoint saves.
#[derive(Clone)]
pub struct CheckpointManager {
    /// Directory to store checkpoints
    checkpoint_dir: PathBuf,
    /// Save checkpoint every N epochs
    save_frequency: u32,
    /// Maximum number of checkpoints to keep
    max_checkpoints: usize,
    /// Adapter ID for this training session
    adapter_id: String,
    /// Optional signing keypair for checkpoint integrity
    signing_key: Option<Keypair>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new<P: AsRef<Path>>(
        checkpoint_dir: P,
        save_frequency: u32,
        max_checkpoints: usize,
        adapter_id: String,
    ) -> Self {
        Self {
            checkpoint_dir: checkpoint_dir.as_ref().to_path_buf(),
            save_frequency,
            max_checkpoints,
            adapter_id,
            signing_key: None,
        }
    }

    /// Create a new checkpoint manager with a signing keypair for integrity verification
    pub fn new_with_signing_key<P: AsRef<Path>>(
        checkpoint_dir: P,
        save_frequency: u32,
        max_checkpoints: usize,
        adapter_id: String,
        signing_key: Keypair,
    ) -> Self {
        Self {
            checkpoint_dir: checkpoint_dir.as_ref().to_path_buf(),
            save_frequency,
            max_checkpoints,
            adapter_id,
            signing_key: Some(signing_key),
        }
    }

    /// Check if checkpoint should be saved at this epoch
    pub fn should_save(&self, epoch: u32) -> bool {
        epoch > 0 && epoch.is_multiple_of(self.save_frequency)
    }

    /// Get checkpoint path for a specific epoch
    pub fn checkpoint_path(&self, epoch: u32) -> PathBuf {
        self.checkpoint_dir
            .join(format!("{}_epoch_{:04}.ckpt", self.adapter_id, epoch))
    }

    /// Get path for latest checkpoint
    pub fn latest_checkpoint_path(&self) -> PathBuf {
        self.checkpoint_dir
            .join(format!("{}_latest.ckpt", self.adapter_id))
    }

    /// Save checkpoint
    pub async fn save_checkpoint(&self, checkpoint: &TrainingCheckpoint) -> Result<()> {
        // Save to epoch-specific file
        let epoch_path = self.checkpoint_path(checkpoint.epoch);
        checkpoint
            .save(&epoch_path, self.signing_key.as_ref())
            .await?;

        // Save to latest checkpoint (for easy resumption)
        let latest_path = self.latest_checkpoint_path();
        checkpoint
            .save(&latest_path, self.signing_key.as_ref())
            .await?;

        // Clean up old checkpoints
        self.cleanup_old_checkpoints().await?;

        Ok(())
    }

    /// Load latest checkpoint
    pub async fn load_latest(&self) -> Result<TrainingCheckpoint> {
        let latest_path = self.latest_checkpoint_path();
        TrainingCheckpoint::load(latest_path).await
    }

    /// Load checkpoint from specific epoch
    pub async fn load_checkpoint(&self, epoch: u32) -> Result<TrainingCheckpoint> {
        let path = self.checkpoint_path(epoch);
        TrainingCheckpoint::load(path).await
    }

    /// Check if latest checkpoint exists
    pub async fn has_checkpoint(&self) -> bool {
        let latest_path = self.latest_checkpoint_path();
        tokio::fs::metadata(&latest_path).await.is_ok()
    }

    /// List all available checkpoints
    pub async fn list_checkpoints(&self) -> Result<Vec<u32>> {
        let mut epochs = Vec::new();

        let mut dir = tokio::fs::read_dir(&self.checkpoint_dir)
            .await
            .map_err(|e| {
                AosError::Training(format!("Failed to read checkpoint directory: {}", e))
            })?;

        while let Some(entry) = dir.next_entry().await.map_err(|e| {
            AosError::Training(format!("Failed to iterate checkpoint directory: {}", e))
        })? {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();

            // Parse epoch from filename: {adapter_id}_epoch_{epoch:04}.ckpt
            if file_name_str.ends_with(".ckpt") && file_name_str.contains("_epoch_") {
                if let Some(epoch_str) = file_name_str.split("_epoch_").nth(1) {
                    if let Some(epoch_num) = epoch_str.strip_suffix(".ckpt") {
                        if let Ok(epoch) = epoch_num.parse::<u32>() {
                            epochs.push(epoch);
                        }
                    }
                }
            }
        }

        epochs.sort();
        debug!(
            checkpoint_dir = %self.checkpoint_dir.display(),
            count = epochs.len(),
            "Found {} checkpoints",
            epochs.len()
        );

        Ok(epochs)
    }

    /// Delete old checkpoints, keeping only the most recent N
    async fn cleanup_old_checkpoints(&self) -> Result<()> {
        let mut checkpoints = self.list_checkpoints().await?;

        if checkpoints.len() <= self.max_checkpoints {
            return Ok(());
        }

        // Sort in descending order (newest first)
        checkpoints.sort_by(|a, b| b.cmp(a));

        // Delete old checkpoints and their signature sidecars
        for epoch in checkpoints.iter().skip(self.max_checkpoints) {
            let path = self.checkpoint_path(*epoch);
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::warn!(
                    path = %path.display(),
                    epoch = epoch,
                    error = %e,
                    "Failed to delete old checkpoint (non-fatal)"
                );
            } else {
                debug!(
                    path = %path.display(),
                    epoch = epoch,
                    "Deleted old checkpoint"
                );
            }
            // Also remove the sidecar if present
            let sig = sig_path_for(&path);
            let _ = tokio::fs::remove_file(&sig).await;
        }

        Ok(())
    }

    /// Delete all checkpoints for this adapter
    pub async fn delete_all(&self) -> Result<()> {
        let checkpoints = self.list_checkpoints().await?;

        for epoch in checkpoints {
            let path = self.checkpoint_path(epoch);
            tokio::fs::remove_file(&path)
                .await
                .map_err(|e| AosError::Training(format!("Failed to delete checkpoint: {}", e)))?;
            // Also remove signature sidecar
            let _ = tokio::fs::remove_file(sig_path_for(&path)).await;
        }

        // Also delete latest checkpoint and its sidecar
        let latest_path = self.latest_checkpoint_path();
        if tokio::fs::metadata(&latest_path).await.is_ok() {
            tokio::fs::remove_file(&latest_path).await.map_err(|e| {
                AosError::Training(format!("Failed to delete latest checkpoint: {}", e))
            })?;
            let _ = tokio::fs::remove_file(sig_path_for(&latest_path)).await;
        }

        info!(
            adapter_id = %self.adapter_id,
            "Deleted all checkpoints"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_storage::platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        tempfile::Builder::new()
            .prefix("aos-test-")
            .tempdir()
            .expect("failed to create temporary directory for checkpoint test - filesystem may be full or permissions denied")
    }

    fn test_config() -> TrainingConfig {
        TrainingConfig {
            rank: 8,
            alpha: 16.0,
            learning_rate: 0.001,
            batch_size: 32,
            epochs: 10,
            hidden_dim: 768,
            vocab_size: 32000,
            coreml_placement: None,
            preferred_backend: None,
            backend_policy: None,
            coreml_fallback_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
            max_tokens_per_batch: None,
            device_policy: None,
            checkpoint_interval: Some(5),
            warmup_steps: None,
            max_seq_length: None,
            gradient_accumulation_steps: None,
            early_stopping: None,
            patience: None,
            min_delta: None,
            determinism: None,
            moe_config: None,
            use_gpu_backward: false,
            optimizer_config: Default::default(),
            base_model_path: None,
            hidden_state_layer: None,
            validation_split: 0.0,
            preprocessing: None,
            training_contract_version: adapteros_types::training::TRAINING_DATA_CONTRACT_VERSION
                .to_string(),
            pad_token_id: 0,
            ignore_index: -1,
            targets: Vec::new(),
            multi_module_training: false,
            lora_layer_indices: Vec::new(),
            mlx_version: None,
        }
    }

    fn test_weights() -> LoRAWeights {
        LoRAWeights {
            lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
            modules: BTreeMap::new(),
            moe_config: None,
            precomputed_delta: None,
        }
    }

    #[tokio::test]
    async fn test_checkpoint_save_load() {
        let temp_dir = new_test_tempdir();
        let checkpoint_path = temp_dir.path().join("test.ckpt");

        let config = test_config();
        let weights = test_weights();

        let checkpoint =
            TrainingCheckpoint::new(5, 100, 0.5, 0.001, config.clone(), weights.clone());

        // Save checkpoint (unsigned)
        checkpoint.save(&checkpoint_path, None).await.unwrap();

        // Load checkpoint
        let loaded = TrainingCheckpoint::load(&checkpoint_path).await.unwrap();

        assert_eq!(loaded.epoch, 5);
        assert_eq!(loaded.step, 100);
        assert_eq!(loaded.loss, 0.5);
        assert_eq!(loaded.config.rank, 8);
        assert_eq!(loaded.weights.lora_a.len(), 2);
    }

    #[tokio::test]
    async fn test_signed_checkpoint_roundtrip() {
        let temp_dir = new_test_tempdir();
        let checkpoint_path = temp_dir.path().join("signed.ckpt");
        let keypair = Keypair::generate();

        let checkpoint =
            TrainingCheckpoint::new(3, 50, 0.25, 0.0005, test_config(), test_weights());

        // Save with signing
        checkpoint
            .save(&checkpoint_path, Some(&keypair))
            .await
            .unwrap();

        // Verify sidecar was created
        let sig_file = sig_path_for(&checkpoint_path);
        assert!(
            tokio::fs::metadata(&sig_file).await.is_ok(),
            "Signature sidecar should exist"
        );

        // Load should succeed (signature is valid)
        let loaded = TrainingCheckpoint::load(&checkpoint_path).await.unwrap();
        assert_eq!(loaded.epoch, 3);
        assert_eq!(loaded.step, 50);
    }

    #[tokio::test]
    async fn test_tampered_checkpoint_detected() {
        let temp_dir = new_test_tempdir();
        let checkpoint_path = temp_dir.path().join("tampered.ckpt");
        let keypair = Keypair::generate();

        let checkpoint = TrainingCheckpoint::new(1, 10, 0.9, 0.001, test_config(), test_weights());

        // Save signed
        checkpoint
            .save(&checkpoint_path, Some(&keypair))
            .await
            .unwrap();

        // Tamper with the checkpoint file (change a byte)
        let mut bytes = tokio::fs::read(&checkpoint_path).await.unwrap();
        if let Some(b) = bytes.iter_mut().find(|b| **b == b'1') {
            *b = b'2';
        }
        tokio::fs::write(&checkpoint_path, &bytes).await.unwrap();

        // In debug mode, load succeeds with warning; we test that the
        // signature verification itself detects tampering.
        let sig_bytes = tokio::fs::read(sig_path_for(&checkpoint_path))
            .await
            .unwrap();
        let sig = CheckpointSignature::from_json(&sig_bytes).unwrap();
        assert!(
            sig.verify(&bytes).is_err(),
            "Tampered content should fail signature verification"
        );
    }

    #[tokio::test]
    async fn test_missing_sig_in_debug_mode_warns_but_loads() {
        // In debug builds (cfg(debug_assertions)), a missing .sig file should
        // still allow loading.
        let temp_dir = new_test_tempdir();
        let checkpoint_path = temp_dir.path().join("nosig.ckpt");

        let checkpoint = TrainingCheckpoint::new(2, 0, 0.6, 0.001, test_config(), test_weights());

        // Save without signing
        checkpoint.save(&checkpoint_path, None).await.unwrap();

        // In debug builds this should succeed
        #[cfg(debug_assertions)]
        {
            let loaded = TrainingCheckpoint::load(&checkpoint_path).await.unwrap();
            assert_eq!(loaded.epoch, 2);
        }
    }

    #[tokio::test]
    async fn test_checkpoint_signature_struct_roundtrip() {
        let keypair = Keypair::generate();
        let content = b"some checkpoint json content here";

        let sig = CheckpointSignature::sign(content, &keypair);
        assert!(sig.verify(content).is_ok());

        // Serialize and deserialize
        let json = sig.to_json().unwrap();
        let restored = CheckpointSignature::from_json(&json).unwrap();
        assert!(restored.verify(content).is_ok());

        // Different content should fail
        assert!(restored.verify(b"tampered content").is_err());
    }

    #[tokio::test]
    async fn test_checkpoint_manager() {
        let temp_dir = new_test_tempdir();
        let manager = CheckpointManager::new(temp_dir.path(), 2, 3, "test-adapter".to_string());

        let config = TrainingConfig::default();
        let weights = LoRAWeights {
            lora_a: vec![vec![1.0]],
            lora_b: vec![vec![2.0]],
            modules: BTreeMap::new(),
            moe_config: None,
            precomputed_delta: None,
        };

        // Create checkpoints for epochs 2, 4, 6, 8
        for epoch in vec![2, 4, 6, 8] {
            let checkpoint =
                TrainingCheckpoint::new(epoch, 0, 0.5, 0.001, config.clone(), weights.clone());
            manager.save_checkpoint(&checkpoint).await.unwrap();
        }

        // Should have latest checkpoint
        assert!(manager.has_checkpoint().await);

        // Should have max 3 checkpoints (plus latest)
        let checkpoints = manager.list_checkpoints().await.unwrap();
        assert!(checkpoints.len() <= 3);

        // Latest should be epoch 8
        let latest = manager.load_latest().await.unwrap();
        assert_eq!(latest.epoch, 8);
    }

    #[tokio::test]
    async fn test_signed_checkpoint_manager() {
        let temp_dir = new_test_tempdir();
        let keypair = Keypair::generate();
        let manager = CheckpointManager::new_with_signing_key(
            temp_dir.path(),
            1,
            5,
            "signed-adapter".to_string(),
            keypair,
        );

        let checkpoint = TrainingCheckpoint::new(1, 0, 0.4, 0.001, test_config(), test_weights());

        manager.save_checkpoint(&checkpoint).await.unwrap();

        // Both checkpoint files and their sidecars should exist
        let epoch_path = manager.checkpoint_path(1);
        let latest_path = manager.latest_checkpoint_path();
        assert!(tokio::fs::metadata(&epoch_path).await.is_ok());
        assert!(tokio::fs::metadata(sig_path_for(&epoch_path)).await.is_ok());
        assert!(tokio::fs::metadata(&latest_path).await.is_ok());
        assert!(tokio::fs::metadata(sig_path_for(&latest_path))
            .await
            .is_ok());

        // Load should verify and succeed
        let loaded = manager.load_latest().await.unwrap();
        assert_eq!(loaded.epoch, 1);
    }

    #[test]
    fn test_should_save() {
        let temp_dir = new_test_tempdir();
        let manager = CheckpointManager::new(temp_dir.path(), 5, 3, "test".to_string());

        assert!(!manager.should_save(0));
        assert!(!manager.should_save(1));
        assert!(manager.should_save(5));
        assert!(manager.should_save(10));
        assert!(!manager.should_save(11));
    }

    #[test]
    fn test_sig_path_derivation() {
        let ckpt = PathBuf::from("/var/checkpoints/adapter_epoch_0001.ckpt");
        let sig = sig_path_for(&ckpt);
        assert_eq!(
            sig,
            PathBuf::from("/var/checkpoints/adapter_epoch_0001.ckpt.sig")
        );
    }
}
