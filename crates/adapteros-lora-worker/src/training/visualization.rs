//! Training progress visualization helpers
//!
//! Provides ASCII and JSON-based visualizations for training metrics:
//! - Loss curve ASCII charts
//! - Gradient norm monitoring
//! - Learning rate schedule visualization
//! - Real-time training progress indicators

use crate::training::metrics::{MetricsSnapshot, TrainingMetrics, TrainingReport};
use serde::{Deserialize, Serialize};

/// ASCII chart generator for training metrics
pub struct TrainingCharts;

impl TrainingCharts {
    /// Generate a simple ASCII loss curve chart
    pub fn loss_curve_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String {
        let losses = metrics.loss_curve();
        if losses.is_empty() {
            return "No data to chart".to_string();
        }

        Self::ascii_line_chart(&losses, width, height, "Loss Curve")
    }

    /// Generate an ASCII gradient norm chart
    pub fn gradient_norm_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String {
        let norms = metrics.gradient_norm_curve();
        if norms.is_empty() {
            return "No gradient data to chart".to_string();
        }

        Self::ascii_line_chart(&norms, width, height, "Gradient Norm")
    }

    /// Generate an ASCII learning rate history chart
    pub fn learning_rate_chart(metrics: &TrainingMetrics, width: usize, height: usize) -> String {
        let lrs = metrics.learning_rate_history();
        if lrs.is_empty() {
            return "No learning rate data to chart".to_string();
        }

        Self::ascii_line_chart(&lrs, width, height, "Learning Rate")
    }

    /// Generic ASCII line chart generator
    fn ascii_line_chart(data: &[f32], width: usize, height: usize, title: &str) -> String {
        if data.is_empty() {
            return format!("{}: No data", title);
        }

        let min = data.iter().copied().fold(f32::INFINITY, f32::min);
        let max = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let range = if (max - min).abs() < 1e-6 {
            1.0
        } else {
            max - min
        };

        // Create chart grid
        let mut chart = vec![vec![" "; width]; height];

        // Downsample data if necessary
        let step = if data.len() > width {
            data.len() / width
        } else {
            1
        };
        let _x_max = (data.len() - 1).min(width - 1);

        for (i, &value) in data.iter().enumerate() {
            let x = if data.len() > width {
                (i / step).min(width - 1)
            } else {
                i
            };

            // Normalize to height
            let normalized = (value - min) / range;
            let y = (height - 1) - ((normalized * (height - 1) as f32) as usize).min(height - 1);

            if x < width && y < height {
                chart[y][x] = "█";
            }
        }

        // Format output
        let mut result = format!("{}\n", title);
        result.push_str(&format!("Max: {:.6} | Min: {:.6}\n", max, min));
        result.push('+');
        for _ in 0..width {
            result.push('-');
        }
        result.push('+');
        result.push('\n');

        for row in chart {
            result.push('|');
            for cell in row {
                result.push_str(cell);
            }
            result.push('|');
            result.push('\n');
        }

        result.push('+');
        for _ in 0..width {
            result.push('-');
        }
        result.push('+');

        result
    }

    /// Generate a training progress bar
    pub fn progress_bar(current: usize, total: usize, width: usize) -> String {
        if total == 0 {
            return "No progress".to_string();
        }

        let percentage = (current as f32 / total as f32 * 100.0) as usize;
        let filled = (current as f32 / total as f32 * width as f32) as usize;
        let empty = width - filled;

        let mut bar = String::from("[");
        bar.push_str(&"=".repeat(filled));
        bar.push_str(&" ".repeat(empty));
        bar.push_str(&format!("] {}%", percentage));

        bar
    }

    /// Generate a summary card with key metrics
    pub fn summary_card(snapshot: &MetricsSnapshot) -> String {
        let mut card = String::from("+-------- Training Summary --------+\n");
        card.push_str(&format!("|  Epoch:          {:>13}  |\n", snapshot.epoch));
        card.push_str(&format!(
            "|  Loss:           {:>13.6}  |\n",
            snapshot.epoch_loss
        ));
        card.push_str(&format!(
            "|  Loss Trend:     {:>13.6}  |\n",
            snapshot.loss_trend
        ));
        card.push_str(&format!(
            "|  Min/Max Loss:   {:.3} / {:.3}  |\n",
            snapshot.min_loss, snapshot.max_loss
        ));

        if let Some(grad_norm) = snapshot.gradient_norm {
            card.push_str(&format!("|  Grad Norm:      {:>13.6}  |\n", grad_norm));
        }

        card.push_str(&format!(
            "|  Learning Rate:  {:>13.8}  |\n",
            snapshot.learning_rate
        ));
        card.push_str(&format!(
            "|  Throughput:     {:>11.2} b/s |\n",
            snapshot.throughput_bps
        ));
        card.push_str(&format!(
            "|  Batch Time:     {:>10.2} ms |\n",
            snapshot.avg_batch_time_ms
        ));
        card.push_str(&format!(
            "|  Peak Memory:    {:>11.2} MB |\n",
            snapshot.peak_memory_mb
        ));
        card.push_str("+----------------------------------+\n");

        card
    }

    /// Generate detailed training report
    pub fn detailed_report(report: &TrainingReport) -> String {
        let mut report_str = String::from("=== DETAILED TRAINING REPORT ===\n\n");

        // Summary stats
        report_str.push_str(&format!("Total Epochs: {}\n", report.total_epochs));
        report_str.push_str(&format!("Total Batches: {}\n", report.total_batches));
        report_str.push_str(&format!(
            "Training Time: {:.2}s\n\n",
            report.final_snapshot.total_time_ms as f32 / 1000.0
        ));

        // Loss analysis
        report_str.push_str("Loss Analysis:\n");
        report_str.push_str(&format!(
            "  Initial Loss: {:.6}\n",
            report.loss_curve.first().copied().unwrap_or(0.0)
        ));
        report_str.push_str(&format!(
            "  Final Loss: {:.6}\n",
            report.final_snapshot.epoch_loss
        ));
        report_str.push_str(&format!(
            "  Improvement per Epoch: {:.6}\n",
            report.loss_improvement_per_epoch()
        ));
        report_str.push_str(&format!("  Converged: {}\n\n", report.has_converged()));

        // Learning rate analysis
        if !report.learning_rate_history.is_empty() {
            report_str.push_str("Learning Rate History:\n");
            report_str.push_str(&format!(
                "  Initial LR: {:.8}\n",
                report.learning_rate_history[0]
            ));
            report_str.push_str(&format!(
                "  Final LR: {:.8}\n",
                report.learning_rate_history.last().copied().unwrap_or(0.0)
            ));
            report_str.push_str(&format!(
                "  Adjustments: {}\n\n",
                count_lr_adjustments(&report.learning_rate_history)
            ));
        }

        // Gradient analysis
        if !report.gradient_norm_curve.is_empty() {
            let max_grad = report
                .gradient_norm_curve
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let avg_grad = report.gradient_norm_curve.iter().sum::<f32>()
                / report.gradient_norm_curve.len() as f32;

            report_str.push_str("Gradient Analysis:\n");
            report_str.push_str(&format!("  Max Gradient Norm: {:.6}\n", max_grad));
            report_str.push_str(&format!("  Avg Gradient Norm: {:.6}\n", avg_grad));
            report_str.push_str(&format!(
                "  Stability: {}\n\n",
                if max_grad > 100.0 {
                    "Concerning"
                } else {
                    "Stable"
                }
            ));
        }

        // Performance metrics
        report_str.push_str("Performance:\n");
        report_str.push_str(&format!(
            "  Throughput: {:.2} batches/sec\n",
            report.final_snapshot.throughput_bps
        ));
        report_str.push_str(&format!(
            "  Avg Batch Time: {:.2} ms\n",
            report.final_snapshot.avg_batch_time_ms
        ));
        report_str.push_str(&format!(
            "  Peak Memory: {:.2} MB\n",
            report.final_snapshot.peak_memory_mb
        ));

        report_str
    }
}

/// JSON-serializable training progress
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingProgress {
    /// Current epoch (0-indexed)
    pub current_epoch: usize,
    /// Total epochs
    pub total_epochs: usize,
    /// Current batch
    pub current_batch: usize,
    /// Total batches processed
    pub total_batches: usize,
    /// Current loss
    pub current_loss: f32,
    /// Best loss seen so far
    pub best_loss: f32,
    /// Current learning rate
    pub learning_rate: f32,
    /// Estimated time remaining in seconds
    pub eta_seconds: f32,
    /// Percentage complete (0-100)
    pub progress_percent: f32,
}

impl TrainingProgress {
    /// Create from metrics snapshot
    pub fn from_snapshot(snapshot: &MetricsSnapshot, total_epochs: usize) -> Self {
        let progress_percent = if total_epochs > 0 {
            (snapshot.epoch as f32 / total_epochs as f32) * 100.0
        } else {
            0.0
        };

        let eta_seconds = if snapshot.throughput_bps > 0.0 && total_epochs > snapshot.epoch {
            let remaining_batches =
                (total_epochs - snapshot.epoch) as f32 * (snapshot.throughput_bps / 10.0); // Rough estimate
            remaining_batches / snapshot.throughput_bps
        } else {
            0.0
        };

        Self {
            current_epoch: snapshot.epoch,
            total_epochs,
            current_batch: 0,
            total_batches: 0,
            current_loss: snapshot.epoch_loss,
            best_loss: snapshot.min_loss,
            learning_rate: snapshot.learning_rate,
            eta_seconds,
            progress_percent,
        }
    }

    /// Format as human-readable string
    pub fn to_human_readable(&self) -> String {
        let bar = TrainingCharts::progress_bar(self.current_epoch, self.total_epochs, 30);
        let hours = self.eta_seconds as u64 / 3600;
        let minutes = (self.eta_seconds as u64 % 3600) / 60;
        let seconds = self.eta_seconds as u64 % 60;

        format!(
            "Epoch {}/{} {} | Loss: {:.6} (Best: {:.6}) | LR: {:.8} | ETA: {:02}:{:02}:{:02}",
            self.current_epoch,
            self.total_epochs,
            bar,
            self.current_loss,
            self.best_loss,
            self.learning_rate,
            hours,
            minutes,
            seconds
        )
    }
}

/// Helper to count learning rate adjustments
fn count_lr_adjustments(history: &[f32]) -> usize {
    if history.len() < 2 {
        return 0;
    }

    let mut count = 0;
    for i in 1..history.len() {
        // Count distinct values or significant changes (>1% difference)
        let diff = (history[i] - history[i - 1]).abs() / history[i - 1];
        if diff > 0.01 {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar() {
        let bar = TrainingCharts::progress_bar(5, 10, 20);
        assert!(bar.contains("50%"));
        assert!(bar.contains("["));
        assert!(bar.contains("]"));
    }

    #[test]
    fn test_ascii_chart_generation() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let chart = TrainingCharts::ascii_line_chart(&data, 40, 10, "Test Chart");
        assert!(chart.contains("Test Chart"));
        assert!(chart.contains("█"));
        assert!(chart.contains("+"));
    }

    #[test]
    fn test_summary_card() {
        let snapshot = MetricsSnapshot {
            epoch: 5,
            epoch_loss: 0.123,
            avg_batch_loss: 0.125,
            min_loss: 0.100,
            max_loss: 0.150,
            loss_trend: 0.01,
            gradient_norm: Some(0.05),
            learning_rate: 0.0001,
            avg_batch_time_ms: 50.0,
            throughput_bps: 20.0,
            peak_memory_mb: 512.0,
            total_time_ms: 5000,
        };

        let card = TrainingCharts::summary_card(&snapshot);
        assert!(card.contains("Training Summary"));
        assert!(card.contains("Epoch:"));
        assert!(card.contains("5"));
    }

    #[test]
    fn test_training_progress() {
        let snapshot = MetricsSnapshot {
            epoch: 5,
            epoch_loss: 0.123,
            avg_batch_loss: 0.125,
            min_loss: 0.100,
            max_loss: 0.150,
            loss_trend: 0.01,
            gradient_norm: Some(0.05),
            learning_rate: 0.0001,
            avg_batch_time_ms: 50.0,
            throughput_bps: 20.0,
            peak_memory_mb: 512.0,
            total_time_ms: 5000,
        };

        let progress = TrainingProgress::from_snapshot(&snapshot, 10);
        assert_eq!(progress.current_epoch, 5);
        assert_eq!(progress.total_epochs, 10);
        assert!(progress.progress_percent > 0.0);
    }
}
