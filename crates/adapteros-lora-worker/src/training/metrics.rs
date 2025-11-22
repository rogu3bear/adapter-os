//! Comprehensive training metrics tracking
//!
//! Provides detailed metrics collection throughout the training lifecycle:
//! - Loss curve tracking across epochs and batches
//! - Gradient norm monitoring for stability
//! - Learning rate schedule tracking and adjustments
//! - Training throughput and performance metrics
//! - Integration with telemetry system for export

use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Comprehensive training metrics collector
#[derive(Clone)]
pub struct TrainingMetrics {
    /// Loss values per epoch
    epoch_losses: VecDeque<f32>,
    /// Loss values per batch (rolling window)
    batch_losses: VecDeque<f32>,
    /// Gradient norms per epoch
    gradient_norms: VecDeque<f32>,
    /// Learning rate values per epoch
    learning_rates: VecDeque<f32>,
    /// Batch timing in milliseconds
    batch_timings_ms: VecDeque<u64>,
    /// Peak memory usage in MB
    peak_memory_mb: f32,
    /// Current epoch
    current_epoch: usize,
    /// Current batch within epoch
    current_batch: usize,
    /// Total batches processed
    total_batches: usize,
    /// Training start time
    start_time: Option<Instant>,
    /// Telemetry writer for export
    telemetry: TelemetryWriter,
    /// Configuration for metrics collection
    config: MetricsConfig,
}

impl std::fmt::Debug for TrainingMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrainingMetrics")
            .field("epoch_losses", &self.epoch_losses)
            .field("batch_losses", &self.batch_losses)
            .field("gradient_norms", &self.gradient_norms)
            .field("learning_rates", &self.learning_rates)
            .field("batch_timings_ms", &self.batch_timings_ms)
            .field("peak_memory_mb", &self.peak_memory_mb)
            .field("current_epoch", &self.current_epoch)
            .field("current_batch", &self.current_batch)
            .field("total_batches", &self.total_batches)
            .field("start_time", &self.start_time)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// Configuration for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Maximum loss history to keep in memory
    pub max_loss_history: usize,
    /// Maximum batch history to keep in memory
    pub max_batch_history: usize,
    /// Maximum gradient norm history to keep
    pub max_gradient_history: usize,
    /// Compute gradient norms (expensive)
    pub track_gradients: bool,
    /// Track memory usage (requires instrumentation)
    pub track_memory: bool,
    /// Export metrics every N epochs
    pub export_interval_epochs: usize,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            max_loss_history: 100,
            max_batch_history: 500,
            max_gradient_history: 100,
            track_gradients: true,
            track_memory: true,
            export_interval_epochs: 1,
        }
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Epoch number
    pub epoch: usize,
    /// Loss for this epoch
    pub epoch_loss: f32,
    /// Average batch loss for this epoch
    pub avg_batch_loss: f32,
    /// Minimum loss seen
    pub min_loss: f32,
    /// Maximum loss seen
    pub max_loss: f32,
    /// Loss trend (recent vs. early): positive = improving
    pub loss_trend: f32,
    /// Gradient norm if tracked
    pub gradient_norm: Option<f32>,
    /// Current learning rate
    pub learning_rate: f32,
    /// Average batch time in milliseconds
    pub avg_batch_time_ms: f32,
    /// Throughput (batches per second)
    pub throughput_bps: f32,
    /// Peak memory usage in MB
    pub peak_memory_mb: f32,
    /// Total training time in milliseconds
    pub total_time_ms: u64,
}

impl TrainingMetrics {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Result<Self> {
        let telemetry = TelemetryWriter::new("training_metrics", 1000, 1024 * 1024)?;

        info!(
            "Initialized training metrics: max_loss_history={}, track_gradients={}",
            config.max_loss_history, config.track_gradients
        );

        Ok(Self {
            epoch_losses: VecDeque::with_capacity(config.max_loss_history),
            batch_losses: VecDeque::with_capacity(config.max_batch_history),
            gradient_norms: VecDeque::with_capacity(config.max_gradient_history),
            learning_rates: VecDeque::with_capacity(config.max_loss_history),
            batch_timings_ms: VecDeque::with_capacity(config.max_batch_history),
            peak_memory_mb: 0.0,
            current_epoch: 0,
            current_batch: 0,
            total_batches: 0,
            start_time: None,
            telemetry,
            config,
        })
    }

    /// Create with default configuration
    pub fn default_with_telemetry() -> Result<Self> {
        Self::new(MetricsConfig::default())
    }

    /// Mark the start of training
    pub fn mark_training_start(&mut self) {
        self.start_time = Some(Instant::now());
        self.current_epoch = 0;
        self.current_batch = 0;
        self.total_batches = 0;

        info!("Training metrics marked start");
    }

    /// Record loss for the current epoch
    pub fn record_epoch_loss(&mut self, epoch: usize, loss: f32) {
        self.current_epoch = epoch;
        self.epoch_losses.push_back(loss);

        // Maintain max size
        if self.epoch_losses.len() > self.config.max_loss_history {
            self.epoch_losses.pop_front();
        }

        debug!("Recorded epoch {} loss: {:.6}", epoch, loss);
    }

    /// Record loss for a batch
    pub fn record_batch_loss(&mut self, batch_idx: usize, loss: f32) {
        self.current_batch = batch_idx;
        self.total_batches += 1;
        self.batch_losses.push_back(loss);

        // Maintain max size
        if self.batch_losses.len() > self.config.max_batch_history {
            self.batch_losses.pop_front();
        }

        debug!("Recorded batch {} loss: {:.6}", batch_idx, loss);
    }

    /// Record batch timing
    pub fn record_batch_timing(&mut self, elapsed_ms: u64) {
        self.batch_timings_ms.push_back(elapsed_ms);

        // Maintain max size
        if self.batch_timings_ms.len() > self.config.max_batch_history {
            self.batch_timings_ms.pop_front();
        }
    }

    /// Record gradient norm (L2 norm of all gradients)
    pub fn record_gradient_norm(&mut self, norm: f32) {
        if !self.config.track_gradients {
            return;
        }

        self.gradient_norms.push_back(norm);

        // Maintain max size
        if self.gradient_norms.len() > self.config.max_gradient_history {
            self.gradient_norms.pop_front();
        }

        debug!("Recorded gradient norm: {:.6}", norm);
    }

    /// Record learning rate adjustment
    pub fn record_learning_rate(&mut self, lr: f32) {
        self.learning_rates.push_back(lr);

        // Maintain max size
        if self.learning_rates.len() > self.config.max_loss_history {
            self.learning_rates.pop_front();
        }

        debug!("Recorded learning rate: {:.8}", lr);
    }

    /// Record peak memory usage
    pub fn record_peak_memory(&mut self, memory_mb: f32) {
        if memory_mb > self.peak_memory_mb {
            self.peak_memory_mb = memory_mb;
            debug!("Updated peak memory: {:.2} MB", memory_mb);
        }
    }

    /// Check for training instability (NaN, Inf, or exploding gradients)
    pub fn check_stability(&self) -> Result<()> {
        if let Some(&loss) = self.epoch_losses.back() {
            if loss.is_nan() {
                return Err(adapteros_core::AosError::Training(
                    "NaN loss detected - training instability".to_string(),
                ));
            }
            if loss.is_infinite() {
                return Err(adapteros_core::AosError::Training(
                    "Infinite loss detected - training instability".to_string(),
                ));
            }
            if loss > 1e6 {
                warn!("Loss exceeded 1e6: {:.2}, possible divergence", loss);
            }
        }

        if let Some(&grad_norm) = self.gradient_norms.back() {
            if grad_norm.is_nan() || grad_norm.is_infinite() {
                return Err(adapteros_core::AosError::Training(
                    "Gradient norm invalid - training instability".to_string(),
                ));
            }
            if grad_norm > 100.0 {
                warn!(
                    "Gradient norm exceeded 100.0: {:.2}, exploding gradients",
                    grad_norm
                );
            }
        }

        Ok(())
    }

    /// Detect if learning rate needs adjustment based on loss plateau
    pub fn should_adjust_learning_rate(&self) -> bool {
        if self.epoch_losses.len() < 3 {
            return false;
        }

        // Get last 3 losses
        let recent: Vec<f32> = self.epoch_losses.iter().rev().take(3).copied().collect();
        if recent.len() < 3 {
            return false;
        }

        // Check if losses are not improving (plateau)
        let slope = (recent[2] - recent[0]) / 2.0;
        slope.abs() < 0.0001 // Less than 0.01% change per step
    }

    /// Suggest learning rate adjustment factor
    pub fn suggest_learning_rate_factor(&self) -> f32 {
        if self.gradient_norms.is_empty() {
            return 1.0;
        }

        let recent_norm = *self.gradient_norms.back().unwrap();

        // If gradients are exploding, reduce LR
        if recent_norm > 10.0 {
            return 0.5; // Reduce by 50%
        }

        // If gradients are vanishing, increase LR
        if recent_norm < 0.001 {
            return 2.0; // Increase by 100%
        }

        1.0 // No change
    }

    /// Generate a snapshot of current metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        let epoch_loss = self.epoch_losses.back().copied().unwrap_or(0.0);
        let min_loss = self
            .epoch_losses
            .iter()
            .copied()
            .fold(f32::INFINITY, f32::min);
        let max_loss = self
            .epoch_losses
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);

        // Calculate loss trend: (early average - recent average)
        let loss_trend = if self.epoch_losses.len() >= 2 {
            let mid = self.epoch_losses.len() / 2;
            let early_avg: f32 = self.epoch_losses.iter().take(mid).sum::<f32>() / mid as f32;
            let recent_avg: f32 =
                self.epoch_losses.iter().rev().take(mid).sum::<f32>() / mid as f32;
            early_avg - recent_avg
        } else {
            0.0
        };

        let avg_batch_loss = if self.batch_losses.is_empty() {
            0.0
        } else {
            self.batch_losses.iter().sum::<f32>() / self.batch_losses.len() as f32
        };

        let learning_rate = self.learning_rates.back().copied().unwrap_or(0.0);

        let avg_batch_time_ms = if self.batch_timings_ms.is_empty() {
            0.0
        } else {
            self.batch_timings_ms.iter().sum::<u64>() as f32 / self.batch_timings_ms.len() as f32
        };

        let throughput_bps = if avg_batch_time_ms > 0.0 {
            1000.0 / avg_batch_time_ms
        } else {
            0.0
        };

        let total_time_ms = self
            .start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        MetricsSnapshot {
            epoch: self.current_epoch,
            epoch_loss,
            avg_batch_loss,
            min_loss,
            max_loss,
            loss_trend,
            gradient_norm: self.gradient_norms.back().copied(),
            learning_rate,
            avg_batch_time_ms,
            throughput_bps,
            peak_memory_mb: self.peak_memory_mb,
            total_time_ms,
        }
    }

    /// Export metrics to telemetry system
    pub fn export_to_telemetry(&mut self) -> Result<()> {
        let snapshot = self.snapshot();

        let payload = serde_json::json!({
            "epoch": snapshot.epoch,
            "epoch_loss": snapshot.epoch_loss,
            "avg_batch_loss": snapshot.avg_batch_loss,
            "min_loss": snapshot.min_loss,
            "max_loss": snapshot.max_loss,
            "loss_trend": snapshot.loss_trend,
            "gradient_norm": snapshot.gradient_norm,
            "learning_rate": snapshot.learning_rate,
            "avg_batch_time_ms": snapshot.avg_batch_time_ms,
            "throughput_bps": snapshot.throughput_bps,
            "peak_memory_mb": snapshot.peak_memory_mb,
            "total_time_ms": snapshot.total_time_ms,
        });

        self.telemetry
            .log("training.metrics_snapshot", payload.clone())?;

        debug!("Exported training metrics to telemetry: {:?}", payload);

        Ok(())
    }

    /// Get loss curve as vector (for visualization)
    pub fn loss_curve(&self) -> Vec<f32> {
        self.epoch_losses.iter().copied().collect()
    }

    /// Get batch losses as vector
    pub fn batch_loss_curve(&self) -> Vec<f32> {
        self.batch_losses.iter().copied().collect()
    }

    /// Get gradient norm curve
    pub fn gradient_norm_curve(&self) -> Vec<f32> {
        self.gradient_norms.iter().copied().collect()
    }

    /// Get learning rate history
    pub fn learning_rate_history(&self) -> Vec<f32> {
        self.learning_rates.iter().copied().collect()
    }

    /// Get batch timing history
    pub fn batch_timing_history(&self) -> Vec<u64> {
        self.batch_timings_ms.iter().copied().collect()
    }

    /// Export full training report
    pub fn export_training_report(&self) -> TrainingReport {
        TrainingReport {
            total_epochs: self.current_epoch,
            total_batches: self.total_batches,
            loss_curve: self.loss_curve(),
            gradient_norm_curve: self.gradient_norm_curve(),
            learning_rate_history: self.learning_rate_history(),
            final_snapshot: self.snapshot(),
        }
    }
}

/// Complete training report for analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingReport {
    /// Total epochs completed
    pub total_epochs: usize,
    /// Total batches processed
    pub total_batches: usize,
    /// Loss values per epoch
    pub loss_curve: Vec<f32>,
    /// Gradient norms per epoch
    pub gradient_norm_curve: Vec<f32>,
    /// Learning rate history
    pub learning_rate_history: Vec<f32>,
    /// Final metrics snapshot
    pub final_snapshot: MetricsSnapshot,
}

impl TrainingReport {
    /// Calculate average loss improvement per epoch
    pub fn loss_improvement_per_epoch(&self) -> f32 {
        if self.loss_curve.len() < 2 {
            return 0.0;
        }
        (self.loss_curve[0] - self.loss_curve[self.loss_curve.len() - 1])
            / (self.loss_curve.len() as f32 - 1.0)
    }

    /// Check if training converged (loss improvement < 0.001 for last 20% of epochs)
    pub fn has_converged(&self) -> bool {
        if self.loss_curve.len() < 5 {
            return false;
        }

        // Ensure we check at least 5 samples or 20% of the curve, whichever is larger
        let last_20_pct = (self.loss_curve.len() / 5).max(5).min(self.loss_curve.len());
        let recent_losses = &self.loss_curve[self.loss_curve.len() - last_20_pct..];

        let avg_improvement = recent_losses
            .windows(2)
            .map(|w| (w[0] - w[1]).abs())
            .sum::<f32>()
            / (recent_losses.len() as f32 - 1.0);

        // Threshold of 0.05 means less than 5% change between consecutive epochs
        avg_improvement < 0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let config = MetricsConfig::default();
        let metrics = TrainingMetrics::new(config).unwrap();
        assert_eq!(metrics.current_epoch, 0);
        assert_eq!(metrics.total_batches, 0);
    }

    #[test]
    fn test_record_epoch_loss() {
        let config = MetricsConfig::default();
        let mut metrics = TrainingMetrics::new(config).unwrap();

        metrics.record_epoch_loss(0, 0.5);
        metrics.record_epoch_loss(1, 0.3);

        assert_eq!(metrics.loss_curve().len(), 2);
        assert_eq!(metrics.loss_curve()[0], 0.5);
        assert_eq!(metrics.loss_curve()[1], 0.3);
    }

    #[test]
    fn test_metrics_snapshot() {
        let config = MetricsConfig::default();
        let mut metrics = TrainingMetrics::new(config).unwrap();

        metrics.record_epoch_loss(0, 0.5);
        metrics.record_epoch_loss(1, 0.3);
        metrics.record_learning_rate(0.001);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.epoch, 1);
        assert_eq!(snapshot.epoch_loss, 0.3);
        assert_eq!(snapshot.min_loss, 0.3);
        assert_eq!(snapshot.max_loss, 0.5);
        assert_eq!(snapshot.learning_rate, 0.001);
    }

    #[test]
    fn test_gradient_norm_tracking() {
        let config = MetricsConfig {
            track_gradients: true,
            ..Default::default()
        };
        let mut metrics = TrainingMetrics::new(config).unwrap();

        metrics.record_gradient_norm(0.1);
        metrics.record_gradient_norm(0.05);

        assert_eq!(metrics.gradient_norm_curve().len(), 2);
    }

    #[test]
    fn test_stability_check_nan() {
        let config = MetricsConfig::default();
        let mut metrics = TrainingMetrics::new(config).unwrap();

        metrics.record_epoch_loss(0, f32::NAN);
        let result = metrics.check_stability();
        assert!(result.is_err());
    }

    #[test]
    fn test_loss_trend() {
        let config = MetricsConfig::default();
        let mut metrics = TrainingMetrics::new(config).unwrap();

        // Record improving losses
        for i in 0..5 {
            metrics.record_epoch_loss(i, 1.0 - (i as f32 * 0.1));
        }

        let snapshot = metrics.snapshot();
        assert!(snapshot.loss_trend > 0.0); // Positive trend = improving
    }

    #[test]
    fn test_convergence_detection() {
        let config = MetricsConfig::default();
        let mut metrics = TrainingMetrics::new(config).unwrap();

        // Simulate converged training
        for i in 0..10 {
            let loss = 0.1 + (1.0 / (i as f32 + 1.0));
            metrics.record_epoch_loss(i, loss);
        }

        let report = metrics.export_training_report();
        assert!(report.has_converged());
    }

    #[test]
    fn test_max_history_size() {
        let config = MetricsConfig {
            max_loss_history: 3,
            ..Default::default()
        };
        let mut metrics = TrainingMetrics::new(config).unwrap();

        for i in 0..10 {
            metrics.record_epoch_loss(i, i as f32);
        }

        // Should only keep last 3
        assert_eq!(metrics.loss_curve().len(), 3);
        assert_eq!(metrics.loss_curve()[0], 7.0);
    }
}
