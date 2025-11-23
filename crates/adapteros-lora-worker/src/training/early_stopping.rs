//! Early stopping for training optimization
//!
//! Monitors validation loss and stops training when improvements plateau,
//! preventing overfitting and saving computation time.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Early stopping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarlyStoppingConfig {
    /// Number of epochs to wait for improvement before stopping
    pub patience: u32,
    /// Minimum change in loss to be considered an improvement
    pub min_delta: f32,
    /// Whether to monitor training loss (true) or validation loss (false)
    pub monitor_training_loss: bool,
    /// Restore best weights when stopping
    pub restore_best_weights: bool,
}

impl Default for EarlyStoppingConfig {
    fn default() -> Self {
        Self {
            patience: 5,
            min_delta: 0.001,
            monitor_training_loss: true,
            restore_best_weights: true,
        }
    }
}

impl EarlyStoppingConfig {
    /// Create config with custom patience
    pub fn with_patience(patience: u32) -> Self {
        Self {
            patience,
            ..Default::default()
        }
    }

    /// Create config for validation loss monitoring
    pub fn with_validation_loss() -> Self {
        Self {
            monitor_training_loss: false,
            ..Default::default()
        }
    }

    /// Set minimum delta for improvement
    pub fn with_min_delta(mut self, min_delta: f32) -> Self {
        self.min_delta = min_delta;
        self
    }
}

/// Early stopping state
pub struct EarlyStopping {
    config: EarlyStoppingConfig,
    best_loss: f32,
    best_epoch: u32,
    epochs_without_improvement: u32,
    should_stop: bool,
}

impl EarlyStopping {
    /// Create a new early stopping monitor
    pub fn new(config: EarlyStoppingConfig) -> Self {
        Self {
            config,
            best_loss: f32::INFINITY,
            best_epoch: 0,
            epochs_without_improvement: 0,
            should_stop: false,
        }
    }

    /// Check if loss has improved and update state
    ///
    /// Returns true if this is the best loss so far
    pub fn check(&mut self, current_epoch: u32, current_loss: f32) -> bool {
        let improvement = self.best_loss - current_loss;
        let is_improvement = improvement > self.config.min_delta;

        if is_improvement {
            // Loss improved significantly
            debug!(
                epoch = current_epoch,
                current_loss = current_loss,
                best_loss = self.best_loss,
                improvement = improvement,
                "Training loss improved"
            );

            self.best_loss = current_loss;
            self.best_epoch = current_epoch;
            self.epochs_without_improvement = 0;
            true
        } else {
            // No significant improvement
            self.epochs_without_improvement += 1;

            debug!(
                epoch = current_epoch,
                current_loss = current_loss,
                best_loss = self.best_loss,
                epochs_without_improvement = self.epochs_without_improvement,
                patience = self.config.patience,
                "No significant loss improvement"
            );

            if self.epochs_without_improvement >= self.config.patience {
                self.should_stop = true;
                info!(
                    best_epoch = self.best_epoch,
                    best_loss = self.best_loss,
                    epochs_waited = self.epochs_without_improvement,
                    "Early stopping triggered: no improvement for {} epochs",
                    self.config.patience
                );
            }

            false
        }
    }

    /// Check if training should stop
    pub fn should_stop(&self) -> bool {
        self.should_stop
    }

    /// Get the best loss recorded
    pub fn best_loss(&self) -> f32 {
        self.best_loss
    }

    /// Get the epoch with the best loss
    pub fn best_epoch(&self) -> u32 {
        self.best_epoch
    }

    /// Get epochs without improvement
    pub fn epochs_without_improvement(&self) -> u32 {
        self.epochs_without_improvement
    }

    /// Should restore best weights
    pub fn should_restore_best(&self) -> bool {
        self.config.restore_best_weights && self.should_stop
    }

    /// Reset early stopping state
    pub fn reset(&mut self) {
        self.best_loss = f32::INFINITY;
        self.best_epoch = 0;
        self.epochs_without_improvement = 0;
        self.should_stop = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_early_stopping_improvement() {
        let config = EarlyStoppingConfig::with_patience(3);
        let mut early_stop = EarlyStopping::new(config);

        // Epoch 0: Initial loss
        assert!(early_stop.check(0, 1.0));
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.best_loss(), 1.0);

        // Epoch 1: Improvement
        assert!(early_stop.check(1, 0.5));
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.best_loss(), 0.5);

        // Epoch 2: Further improvement
        assert!(early_stop.check(2, 0.3));
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.best_loss(), 0.3);
    }

    #[test]
    fn test_early_stopping_triggers() {
        let config = EarlyStoppingConfig::with_patience(2);
        let mut early_stop = EarlyStopping::new(config);

        // Epoch 0: Initial
        early_stop.check(0, 1.0);
        assert!(!early_stop.should_stop());

        // Epoch 1: No improvement
        early_stop.check(1, 1.0);
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.epochs_without_improvement(), 1);

        // Epoch 2: Still no improvement
        early_stop.check(2, 1.0);
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.epochs_without_improvement(), 2);

        // Epoch 3: Patience exhausted
        early_stop.check(3, 1.0);
        assert!(early_stop.should_stop());
        assert_eq!(early_stop.best_epoch(), 0);
    }

    #[test]
    fn test_min_delta() {
        let config = EarlyStoppingConfig::with_patience(2).with_min_delta(0.1);
        let mut early_stop = EarlyStopping::new(config);

        // Epoch 0: Initial
        early_stop.check(0, 1.0);

        // Epoch 1: Small improvement (< min_delta), doesn't count
        assert!(!early_stop.check(1, 0.95));
        assert_eq!(early_stop.epochs_without_improvement(), 1);

        // Epoch 2: Significant improvement (> min_delta)
        assert!(early_stop.check(2, 0.8));
        assert_eq!(early_stop.epochs_without_improvement(), 0);
        assert_eq!(early_stop.best_loss(), 0.8);
    }

    #[test]
    fn test_reset() {
        let config = EarlyStoppingConfig::with_patience(1);
        let mut early_stop = EarlyStopping::new(config);

        early_stop.check(0, 1.0);
        early_stop.check(1, 1.0);
        early_stop.check(2, 1.0);
        assert!(early_stop.should_stop());

        early_stop.reset();
        assert!(!early_stop.should_stop());
        assert_eq!(early_stop.best_loss(), f32::INFINITY);
        assert_eq!(early_stop.epochs_without_improvement(), 0);
    }

    #[test]
    fn test_restore_best_weights_flag() {
        let config = EarlyStoppingConfig {
            patience: 1,
            min_delta: 0.001,
            monitor_training_loss: true,
            restore_best_weights: true,
        };
        let mut early_stop = EarlyStopping::new(config);

        early_stop.check(0, 1.0);
        early_stop.check(1, 1.0);
        early_stop.check(2, 1.0);

        assert!(early_stop.should_stop());
        assert!(early_stop.should_restore_best());
    }
}
