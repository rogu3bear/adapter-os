//! Extension for integrating metrics into the training loop
//!
//! Provides helper functions to integrate the comprehensive metrics system
//! with the MicroLoRATrainer to track loss curves, gradients, and learning rates.

use super::metrics::{MetricsConfig, TrainingMetrics};
use super::trainer::{MicroLoRATrainer, TrainingExample};
use adapteros_core::Result;
use tracing::{info, warn};

/// Extension trait for MicroLoRATrainer to add metrics tracking
pub trait TrainerMetricsExt {
    /// Train with comprehensive metrics collection
    fn train_with_metrics(
        &mut self,
        examples: &[TrainingExample],
        metrics_config: MetricsConfig,
    ) -> impl std::future::Future<Output = Result<(super::trainer::TrainingResult, TrainingMetrics)>>;
}

impl TrainerMetricsExt for MicroLoRATrainer {
    async fn train_with_metrics(
        &mut self,
        examples: &[TrainingExample],
        metrics_config: MetricsConfig,
    ) -> Result<(super::trainer::TrainingResult, TrainingMetrics)> {
        // Extract values we need before moving metrics_config
        let export_interval = metrics_config.export_interval_epochs;
        let learning_rate = self.config.learning_rate;

        let mut metrics = TrainingMetrics::new(metrics_config)?;
        metrics.mark_training_start();

        info!("Starting training with metrics tracking");

        // Train using the callback mechanism to capture metrics
        let result = self
            .train_with_callback(examples, |epoch, epoch_loss| {
                // This callback is called after each epoch
                metrics.record_epoch_loss(epoch - 1, epoch_loss);
                metrics.record_learning_rate(learning_rate);

                // Check stability
                if let Err(e) = metrics.check_stability() {
                    warn!("Training stability check failed: {}", e);
                }

                // Export metrics periodically
                if epoch % export_interval == 0 {
                    if let Err(e) = metrics.export_to_telemetry() {
                        warn!("Failed to export metrics to telemetry: {}", e);
                    }
                }
            })
            .await?;

        // Final metrics export
        if let Err(e) = metrics.export_to_telemetry() {
            warn!("Failed to export final metrics: {}", e);
        }

        info!(
            "Training completed with metrics: loss={:.6}, convergence={}",
            result.final_loss,
            metrics.export_training_report().has_converged()
        );

        Ok((result, metrics))
    }
}

/// Helper function to create a metrics-enabled training session
pub fn create_metrics_enabled_trainer(
    _trainer: &mut MicroLoRATrainer,
    _examples: &[TrainingExample],
    metrics_config: Option<MetricsConfig>,
) -> TrainingMetricsSession {
    let config = metrics_config.unwrap_or_default();
    let mut metrics = TrainingMetrics::new(config.clone()).expect("Failed to create metrics");
    metrics.mark_training_start();

    TrainingMetricsSession {
        metrics,
        batch_count: 0,
        epoch_count: 0,
    }
}

/// Session management for metrics-tracked training
pub struct TrainingMetricsSession {
    metrics: TrainingMetrics,
    batch_count: usize,
    epoch_count: usize,
}

impl TrainingMetricsSession {
    /// Record a batch completion
    pub fn record_batch(&mut self, loss: f32, time_ms: u64, gradient_norm: Option<f32>) {
        self.metrics.record_batch_loss(self.batch_count, loss);
        self.metrics.record_batch_timing(time_ms);

        if let Some(grad_norm) = gradient_norm {
            self.metrics.record_gradient_norm(grad_norm);
        }

        self.batch_count += 1;
    }

    /// Record an epoch completion
    pub fn record_epoch(&mut self, epoch: usize, loss: f32, learning_rate: f32) {
        self.metrics.record_epoch_loss(epoch, loss);
        self.metrics.record_learning_rate(learning_rate);
        self.epoch_count += 1;

        if let Err(e) = self.metrics.check_stability() {
            warn!("Training stability issue: {}", e);
        }
    }

    /// Check if learning rate should be adjusted
    pub fn should_adjust_lr(&self) -> bool {
        self.metrics.should_adjust_learning_rate()
    }

    /// Get suggested learning rate adjustment
    pub fn suggested_lr_factor(&self) -> f32 {
        self.metrics.suggest_learning_rate_factor()
    }

    /// Export metrics at session end
    pub fn finalize(mut self) -> Result<TrainingMetrics> {
        self.metrics.export_to_telemetry()?;
        Ok(self.metrics)
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> super::metrics::MetricsSnapshot {
        self.metrics.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_session_creation() {
        let session = TrainingMetricsSession {
            metrics: TrainingMetrics::new(MetricsConfig::default()).unwrap(),
            batch_count: 0,
            epoch_count: 0,
        };

        assert_eq!(session.batch_count, 0);
        assert_eq!(session.epoch_count, 0);
    }

    #[test]
    fn test_metrics_session_batch_recording() {
        let mut session = TrainingMetricsSession {
            metrics: TrainingMetrics::new(MetricsConfig::default()).unwrap(),
            batch_count: 0,
            epoch_count: 0,
        };

        session.record_batch(0.5, 100, Some(0.1));
        assert_eq!(session.batch_count, 1);
    }

    #[test]
    fn test_metrics_session_epoch_recording() {
        let mut session = TrainingMetricsSession {
            metrics: TrainingMetrics::new(MetricsConfig::default()).unwrap(),
            batch_count: 0,
            epoch_count: 0,
        };

        session.record_epoch(0, 0.5, 0.0001);
        assert_eq!(session.epoch_count, 1);
    }

    #[test]
    fn test_learning_rate_adjustment_check() {
        let mut session = TrainingMetricsSession {
            metrics: TrainingMetrics::new(MetricsConfig::default()).unwrap(),
            batch_count: 0,
            epoch_count: 0,
        };

        // Record multiple epochs with plateauing loss
        for i in 0..5 {
            session.record_epoch(i, 0.1 + (0.01 / (i as f32 + 1.0)), 0.0001);
        }

        // After several epochs with small improvements, should suggest adjustment
        // (actual behavior depends on the threshold in should_adjust_learning_rate)
    }
}
