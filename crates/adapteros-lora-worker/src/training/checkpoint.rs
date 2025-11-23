//! Training checkpoint management for resumable training
//!
//! Enables saving and restoring training state, allowing training to resume
//! from interruptions or to implement strategies like best-model-restore.

use super::trainer::{LoRAWeights, TrainingConfig};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

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
            config,
            weights,
            best_loss: loss,
            epochs_without_improvement: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Save checkpoint to file
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Training(format!("Failed to create checkpoint directory: {}", e))
            })?;
        }

        // Serialize to JSON
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AosError::Training(format!("Failed to serialize checkpoint: {}", e))
        })?;

        // Write to file
        tokio::fs::write(path, json).await.map_err(|e| {
            AosError::Training(format!("Failed to write checkpoint: {}", e))
        })?;

        info!(
            path = %path.display(),
            epoch = self.epoch,
            loss = self.loss,
            "Checkpoint saved successfully"
        );

        Ok(())
    }

    /// Load checkpoint from file
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Read file
        let json = tokio::fs::read_to_string(path).await.map_err(|e| {
            AosError::Training(format!("Failed to read checkpoint: {}", e))
        })?;

        // Deserialize
        let checkpoint: Self = serde_json::from_str(&json).map_err(|e| {
            AosError::Training(format!("Failed to deserialize checkpoint: {}", e))
        })?;

        info!(
            path = %path.display(),
            epoch = checkpoint.epoch,
            loss = checkpoint.loss,
            "Checkpoint loaded successfully"
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

/// Checkpoint manager for handling multiple checkpoints
pub struct CheckpointManager {
    /// Directory to store checkpoints
    checkpoint_dir: PathBuf,
    /// Save checkpoint every N epochs
    save_frequency: u32,
    /// Maximum number of checkpoints to keep
    max_checkpoints: usize,
    /// Adapter ID for this training session
    adapter_id: String,
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
        }
    }

    /// Check if checkpoint should be saved at this epoch
    pub fn should_save(&self, epoch: u32) -> bool {
        epoch > 0 && epoch % self.save_frequency == 0
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
        checkpoint.save(&epoch_path).await?;

        // Save to latest checkpoint (for easy resumption)
        let latest_path = self.latest_checkpoint_path();
        checkpoint.save(&latest_path).await?;

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

        // Delete old checkpoints
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
        }

        Ok(())
    }

    /// Delete all checkpoints for this adapter
    pub async fn delete_all(&self) -> Result<()> {
        let checkpoints = self.list_checkpoints().await?;

        for epoch in checkpoints {
            let path = self.checkpoint_path(epoch);
            tokio::fs::remove_file(&path).await.map_err(|e| {
                AosError::Training(format!("Failed to delete checkpoint: {}", e))
            })?;
        }

        // Also delete latest checkpoint
        let latest_path = self.latest_checkpoint_path();
        if tokio::fs::metadata(&latest_path).await.is_ok() {
            tokio::fs::remove_file(&latest_path).await.map_err(|e| {
                AosError::Training(format!("Failed to delete latest checkpoint: {}", e))
            })?;
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
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_checkpoint_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint_path = temp_dir.path().join("test.ckpt");

        let config = TrainingConfig {
            rank: 8,
            alpha: 16.0,
            learning_rate: 0.001,
            batch_size: 32,
            epochs: 10,
            hidden_dim: 768,
            preferred_backend: None,
            require_gpu: false,
            max_gpu_memory_mb: 0,
        };

        let weights = LoRAWeights {
            lora_a: vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            lora_b: vec![vec![5.0, 6.0], vec![7.0, 8.0]],
        };

        let checkpoint = TrainingCheckpoint::new(5, 100, 0.5, 0.001, config.clone(), weights.clone());

        // Save checkpoint
        checkpoint.save(&checkpoint_path).await.unwrap();

        // Load checkpoint
        let loaded = TrainingCheckpoint::load(&checkpoint_path).await.unwrap();

        assert_eq!(loaded.epoch, 5);
        assert_eq!(loaded.step, 100);
        assert_eq!(loaded.loss, 0.5);
        assert_eq!(loaded.config.rank, 8);
        assert_eq!(loaded.weights.lora_a.len(), 2);
    }

    #[tokio::test]
    async fn test_checkpoint_manager() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path(), 2, 3, "test-adapter".to_string());

        let config = TrainingConfig::default();
        let weights = LoRAWeights {
            lora_a: vec![vec![1.0]],
            lora_b: vec![vec![2.0]],
        };

        // Create checkpoints for epochs 2, 4, 6, 8
        for epoch in vec![2, 4, 6, 8] {
            let checkpoint = TrainingCheckpoint::new(epoch, 0, 0.5, 0.001, config.clone(), weights.clone());
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

    #[test]
    fn test_should_save() {
        let temp_dir = TempDir::new().unwrap();
        let manager = CheckpointManager::new(temp_dir.path(), 5, 3, "test".to_string());

        assert!(!manager.should_save(0));
        assert!(!manager.should_save(1));
        assert!(manager.should_save(5));
        assert!(manager.should_save(10));
        assert!(!manager.should_save(11));
    }
}
