//! Numerical noise tracking for Metal kernels
//!
//! This module tracks quantization noise and numerical errors between
//! reference (high-precision) and quantized kernel executions.
//!
//! Key features:
//! - Per-kernel epsilon tracking (L2 error, max error)
//! - Reference vs quantized comparison
//! - Threshold violation detection
//! - Integration with trace metadata

use adapteros_core::{AosError, Result};
use adapteros_numerics::noise::{measure_error, EpsilonStats, GlobalStabilityReport, Tensor};
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Configuration for noise tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseTrackingConfig {
    /// Enable noise tracking (default: true)
    pub enabled: bool,
    /// Error threshold for strict mode (default: 1e-6)
    pub error_threshold: f64,
    /// Enable strict mode (panic on threshold violation)
    pub strict_mode: bool,
    /// Enable reference computation (slower but more accurate)
    pub enable_reference: bool,
    /// Maximum number of layers to track per step
    pub max_layers_per_step: usize,
}

impl Default for NoiseTrackingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            error_threshold: 1e-6,
            strict_mode: false,
            enable_reference: false, // Disabled by default for performance
            max_layers_per_step: 10,
        }
    }
}

/// Noise tracker for Metal kernels
pub struct NoiseTracker {
    config: NoiseTrackingConfig,
    layer_stats: HashMap<String, EpsilonStats>,
    global_report: GlobalStabilityReport,
    telemetry: Option<Arc<TelemetryWriter>>,
    step_count: u64,
}

impl NoiseTracker {
    /// Create a new noise tracker
    pub fn new(config: NoiseTrackingConfig, telemetry: Option<Arc<TelemetryWriter>>) -> Self {
        Self {
            config,
            layer_stats: HashMap::new(),
            global_report: GlobalStabilityReport::new(),
            telemetry,
            step_count: 0,
        }
    }

    /// Track numerical error for a kernel layer
    ///
    /// # Arguments
    /// * `layer_id` - Unique identifier for the kernel layer
    /// * `quantized_output` - Output from quantized kernel execution
    /// * `reference_output` - Output from reference (high-precision) execution (optional)
    ///
    /// # Returns
    /// * `Result<()>` - Success or error
    pub fn track_layer_error(
        &mut self,
        layer_id: &str,
        quantized_output: &[f32],
        reference_output: Option<&[f32]>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check if we've exceeded the maximum layers per step
        if self.layer_stats.len() >= self.config.max_layers_per_step {
            debug!(
                "Maximum layers per step reached, skipping noise tracking for layer: {}",
                layer_id
            );
            return Ok(());
        }

        let quantized_tensor = Tensor::new(quantized_output.to_vec(), vec![quantized_output.len()]);

        let epsilon_stats = if let Some(ref_output) = reference_output {
            // Compare against reference
            let reference_tensor = Tensor::new(ref_output.to_vec(), vec![ref_output.len()]);

            measure_error(&reference_tensor, &quantized_tensor, layer_id.to_string())
                .map_err(|e| AosError::Kernel(format!("Noise tracking error: {}", e)))?
        } else {
            // Create a zero-error baseline (for when reference is not available)
            EpsilonStats::new(layer_id.to_string(), 0.0, 0.0, 0.0, quantized_output.len())
        };

        // Check threshold violation
        if epsilon_stats.exceeds_threshold(self.config.error_threshold) {
            let error_msg = format!(
                "Threshold violation in layer {}: L2={:.2e}, max={:.2e}, threshold={:.2e}",
                layer_id,
                epsilon_stats.l2_error,
                epsilon_stats.max_error,
                self.config.error_threshold
            );

            if self.config.strict_mode {
                error!("{}", error_msg);
                return Err(AosError::Kernel(error_msg));
            } else {
                warn!("{}", error_msg);
            }
        }

        // Store statistics
        self.layer_stats
            .insert(layer_id.to_string(), epsilon_stats.clone());

        // Log to telemetry if available
        if let Some(ref telemetry) = self.telemetry {
            use adapteros_telemetry::event::KernelNoiseEvent;

            let event = KernelNoiseEvent::new(
                layer_id.to_string(),
                epsilon_stats.l2_error,
                epsilon_stats.max_error,
                epsilon_stats.mean_error,
                epsilon_stats.element_count,
                self.config.error_threshold,
                self.step_count,
            );

            let _ = telemetry.log_kernel_noise(event);
        }

        debug!(
            "Tracked noise for layer {}: L2={:.2e}, max={:.2e}",
            layer_id, epsilon_stats.l2_error, epsilon_stats.max_error
        );

        Ok(())
    }

    /// Track error for a complete kernel step
    ///
    /// This method should be called after each kernel execution step
    /// to aggregate statistics and generate reports.
    pub fn track_step(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.step_count += 1;

        // Aggregate statistics for this step
        let step_stats: Vec<EpsilonStats> = self.layer_stats.values().cloned().collect();
        if !step_stats.is_empty() {
            self.global_report = adapteros_numerics::noise::aggregate_stats(&step_stats);
        }

        // Log step summary
        debug!(
            "Step {} noise summary: {} layers, total L2={:.2e}, max={:.2e}",
            self.step_count,
            self.layer_stats.len(),
            self.global_report.total_l2_error,
            self.global_report.max_layer_error
        );

        // Log to telemetry if available
        if let Some(ref telemetry) = self.telemetry {
            use adapteros_telemetry::event::KernelStepEvent;

            let event = KernelStepEvent::new(
                self.step_count,
                self.layer_stats.len(),
                self.global_report.total_l2_error,
                self.global_report.max_layer_error,
                self.global_report.mean_layer_error,
                self.global_report.stability_score(),
                self.global_report.threshold_violations.clone(),
            );

            let _ = telemetry.log_kernel_step(event);
        }

        // Clear layer stats for next step
        self.layer_stats.clear();

        Ok(())
    }

    /// Get the current global stability report
    pub fn get_stability_report(&self) -> &GlobalStabilityReport {
        &self.global_report
    }

    /// Get statistics for a specific layer
    pub fn get_layer_stats(&self, layer_id: &str) -> Option<&EpsilonStats> {
        self.layer_stats.get(layer_id)
    }

    /// Check if the system is currently stable
    pub fn is_stable(&self) -> bool {
        self.global_report.is_stable(self.config.error_threshold)
    }

    /// Get the current step count
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Reset the tracker (clear all statistics)
    pub fn reset(&mut self) {
        self.layer_stats.clear();
        self.global_report = GlobalStabilityReport::new();
        self.step_count = 0;
    }

    /// Update configuration
    pub fn update_config(&mut self, config: NoiseTrackingConfig) {
        self.config = config;
    }
}

/// Helper function to extract tensor data from Metal buffer
///
/// This is a placeholder implementation. In a real implementation,
/// this would read data from the Metal buffer and convert it to
/// the appropriate format for noise tracking.
pub fn extract_buffer_data(_buffer: &metal::Buffer, length: usize) -> Result<Vec<f32>> {
    // In a real implementation, this would:
    // 1. Map the Metal buffer to CPU-accessible memory
    // 2. Convert from the buffer's format (e.g., FP16) to f32
    // 3. Return the data as a Vec<f32>

    // For now, return a placeholder implementation
    Ok(vec![0.0; length])
}

/// Helper function to create reference tensor data
///
/// This function generates high-precision reference data for comparison.
/// In a real implementation, this might involve:
/// - Running the kernel with higher precision
/// - Using a different quantization scheme
/// - Computing exact mathematical results
pub fn create_reference_data(quantized_data: &[f32]) -> Vec<f32> {
    // For now, return the quantized data as-is
    // In a real implementation, this would generate high-precision reference
    quantized_data.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_tracker_creation() {
        let config = NoiseTrackingConfig::default();
        let tracker = NoiseTracker::new(config, None);

        assert_eq!(tracker.step_count(), 0);
        assert!(tracker.is_stable());
    }

    #[test]
    fn test_track_layer_error() {
        let config = NoiseTrackingConfig::default();
        let mut tracker = NoiseTracker::new(config, None);

        let quantized = vec![1.0, 2.0, 3.0];
        let reference = vec![1.01, 1.99, 3.01];

        let result = tracker.track_layer_error("test_layer", &quantized, Some(&reference));
        assert!(result.is_ok());

        let stats = tracker.get_layer_stats("test_layer");
        assert!(stats.is_some());

        let stats = stats.unwrap();
        assert_eq!(stats.layer_id, "test_layer");
        assert!(stats.l2_error > 0.0);
    }

    #[test]
    fn test_track_step() {
        let config = NoiseTrackingConfig::default();
        let mut tracker = NoiseTracker::new(config, None);

        // Add some layer statistics
        let quantized = vec![1.0, 2.0, 3.0];
        let reference = vec![1.01, 1.99, 3.01];

        tracker
            .track_layer_error("layer1", &quantized, Some(&reference))
            .unwrap();
        tracker
            .track_layer_error("layer2", &quantized, Some(&reference))
            .unwrap();

        // Track step
        let result = tracker.track_step();
        assert!(result.is_ok());

        assert_eq!(tracker.step_count(), 1);
        assert_eq!(tracker.layer_stats.len(), 0); // Cleared after step
    }

    #[test]
    fn test_threshold_violation_strict_mode() {
        let mut config = NoiseTrackingConfig::default();
        config.strict_mode = true;
        config.error_threshold = 1e-10; // Very strict threshold

        let mut tracker = NoiseTracker::new(config, None);

        let quantized = vec![1.0, 2.0, 3.0];
        let reference = vec![2.0, 3.0, 4.0]; // Large difference

        let result = tracker.track_layer_error("test_layer", &quantized, Some(&reference));
        assert!(result.is_err());
    }

    #[test]
    fn test_threshold_violation_warning_mode() {
        let mut config = NoiseTrackingConfig::default();
        config.strict_mode = false;
        config.error_threshold = 1e-10; // Very strict threshold

        let mut tracker = NoiseTracker::new(config, None);

        let quantized = vec![1.0, 2.0, 3.0];
        let reference = vec![2.0, 3.0, 4.0]; // Large difference

        let result = tracker.track_layer_error("test_layer", &quantized, Some(&reference));
        assert!(result.is_ok()); // Should not panic in warning mode
    }

    #[test]
    fn test_disabled_tracking() {
        let mut config = NoiseTrackingConfig::default();
        config.enabled = false;

        let mut tracker = NoiseTracker::new(config, None);

        let quantized = vec![1.0, 2.0, 3.0];
        let reference = vec![2.0, 3.0, 4.0];

        let result = tracker.track_layer_error("test_layer", &quantized, Some(&reference));
        assert!(result.is_ok());

        // Should not have any statistics
        assert!(tracker.get_layer_stats("test_layer").is_none());
    }
}
