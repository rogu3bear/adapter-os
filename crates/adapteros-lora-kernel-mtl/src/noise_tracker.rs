//! Numerical noise tracking utilities for Metal kernels.

use adapteros_core::{AosError, Result};
use adapteros_numerics::noise::{
    aggregate_stats, measure_error, EpsilonStats, GlobalStabilityReport, Tensor,
};
use adapteros_telemetry::TelemetryWriter;
use half::f16;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, warn};

const REFERENCE_WEIGHTS: [f64; 5] = [0.0625, 0.25, 0.375, 0.25, 0.0625];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseTrackingConfig {
    pub enabled: bool,
    pub error_threshold: f64,
    pub strict_mode: bool,
    pub enable_reference: bool,
    pub max_layers_per_step: usize,
}

impl Default for NoiseTrackingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            error_threshold: 1e-6,
            strict_mode: false,
            enable_reference: false,
            max_layers_per_step: 10,
        }
    }
}

pub struct NoiseTracker {
    config: NoiseTrackingConfig,
    layer_stats: HashMap<String, EpsilonStats>,
    global_report: GlobalStabilityReport,
    telemetry: Option<Arc<TelemetryWriter>>,
    step_count: u64,
}

impl NoiseTracker {
    pub fn new(config: NoiseTrackingConfig, telemetry: Option<Arc<TelemetryWriter>>) -> Self {
        Self {
            config,
            layer_stats: HashMap::new(),
            global_report: GlobalStabilityReport::new(),
            telemetry,
            step_count: 0,
        }
    }

    pub fn track_buffers(
        &mut self,
        layer_id: &str,
        quantized_buffer: &metal::Buffer,
        reference_buffer: Option<&metal::Buffer>,
        element_count: usize,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let quantized = extract_buffer_data(quantized_buffer, element_count)?;
        let reference = match (reference_buffer, self.config.enable_reference) {
            (Some(buf), _) => Some(extract_buffer_data(buf, element_count)?),
            (None, true) => Some(create_reference_data(&quantized)),
            _ => None,
        };

        self.track_layer_error(layer_id, &quantized, reference.as_deref())
    }

    pub fn track_layer_error(
        &mut self,
        layer_id: &str,
        quantized_output: &[f32],
        reference_output: Option<&[f32]>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        if self.layer_stats.len() >= self.config.max_layers_per_step {
            debug!(
                "Maximum layers per step reached, skipping noise tracking for layer: {}",
                layer_id
            );
            return Ok(());
        }

        let quantized_tensor = Tensor::new(quantized_output.to_vec(), vec![quantized_output.len()]);
        let epsilon_stats = if let Some(reference) = reference_output {
            let reference_tensor = Tensor::new(reference.to_vec(), vec![reference.len()]);
            measure_error(&reference_tensor, &quantized_tensor, layer_id.to_string())
                .map_err(|e| AosError::Kernel(format!("Noise tracking error: {}", e)))?
        } else {
            EpsilonStats::new(layer_id.to_string(), 0.0, 0.0, 0.0, quantized_output.len())
        };

        if epsilon_stats.exceeds_threshold(self.config.error_threshold) {
            let msg = format!(
                "Threshold violation in layer {}: L2={:.2e}, max={:.2e}, threshold={:.2e}",
                layer_id,
                epsilon_stats.l2_error,
                epsilon_stats.max_error,
                self.config.error_threshold
            );
            if self.config.strict_mode {
                error!("{}", msg);
                return Err(AosError::Kernel(msg));
            } else {
                warn!("{}", msg);
            }
        }

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

        self.layer_stats
            .insert(layer_id.to_string(), epsilon_stats.clone());

        debug!(
            "Tracked noise for layer {}: L2={:.2e}, max={:.2e}",
            layer_id, epsilon_stats.l2_error, epsilon_stats.max_error
        );

        Ok(())
    }

    pub fn track_step(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        self.step_count += 1;

        if !self.layer_stats.is_empty() {
            let stats: Vec<EpsilonStats> = self.layer_stats.values().cloned().collect();
            self.global_report = aggregate_stats(&stats);
        }

        debug!(
            "Step {} noise summary: {} layers, total L2={:.2e}, max={:.2e}",
            self.step_count,
            self.layer_stats.len(),
            self.global_report.total_l2_error,
            self.global_report.max_layer_error
        );

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

        self.layer_stats.clear();
        Ok(())
    }

    pub fn get_stability_report(&self) -> &GlobalStabilityReport {
        &self.global_report
    }

    pub fn get_layer_stats(&self, layer_id: &str) -> Option<&EpsilonStats> {
        self.layer_stats.get(layer_id)
    }

    pub fn is_stable(&self) -> bool {
        self.global_report.is_stable(self.config.error_threshold)
    }

    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    pub fn reset(&mut self) {
        self.layer_stats.clear();
        self.global_report = GlobalStabilityReport::new();
        self.step_count = 0;
    }

    pub fn update_config(&mut self, config: NoiseTrackingConfig) {
        self.config = config;
    }
}

pub fn extract_buffer_data(buffer: &metal::Buffer, length: usize) -> Result<Vec<f32>> {
    if length == 0 {
        return Ok(Vec::new());
    }

    let byte_len = buffer.length() as usize;
    let ptr = buffer.contents();
    if ptr.is_null() {
        return Err(AosError::Kernel(
            "Metal buffer contents pointer is null".into(),
        ));
    }

    let f32_bytes = length
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| {
            AosError::Kernel("Element count overflow while reading Metal buffer".into())
        })?;
    let f16_bytes = length
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| {
            AosError::Kernel("Element count overflow while reading Metal buffer".into())
        })?;

    unsafe {
        if byte_len >= f32_bytes {
            let slice = std::slice::from_raw_parts(ptr as *const f32, length);
            Ok(slice.to_vec())
        } else if byte_len >= f16_bytes {
            let slice = std::slice::from_raw_parts(ptr as *const u16, length);
            Ok(slice
                .iter()
                .map(|&bits| f16::from_bits(bits).to_f32())
                .collect())
        } else {
            Err(AosError::Kernel(format!(
                "Metal buffer ({} bytes) too small for {} elements",
                byte_len, length
            )))
        }
    }
}

pub fn create_reference_data(quantized_data: &[f32]) -> Vec<f32> {
    if quantized_data.is_empty() {
        return Vec::new();
    }

    let len = quantized_data.len();
    let last = len - 1;
    let mut output = Vec::with_capacity(len);

    for i in 0..len {
        let mut acc = 0.0_f64;
        let mut c = 0.0_f64;
        for (idx, weight) in reference_indices(i, last)
            .into_iter()
            .zip(REFERENCE_WEIGHTS.iter())
        {
            let term = (quantized_data[idx] as f64) * weight;
            let y = term - c;
            let t = acc + y;
            c = (t - acc) - y;
            acc = t;
        }
        output.push(acc as f32);
    }

    output
}

fn reference_indices(center: usize, last: usize) -> [usize; 5] {
    [
        center.saturating_sub(2),
        center.saturating_sub(1),
        center,
        center.saturating_add(1).min(last),
        center.saturating_add(2).min(last),
    ]
}
