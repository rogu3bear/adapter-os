//! Quantization Utilities for CoreML Models
//!
//! This module provides quantization support for converting models to CoreML
//! with various precision levels (FP16, INT8, INT4) while maintaining ANE compatibility.
//!
//! ## Quantization Strategies
//!
//! - **FP16**: Recommended for ANE, 2x memory reduction, minimal accuracy loss
//! - **INT8**: 4x memory reduction, calibration required for accuracy
//! - **INT4**: 8x memory reduction, experimental, may not be ANE-compatible
//!
//! ## ANE Compatibility
//!
//! Apple Neural Engine supports:
//! - FP16: Full support
//! - INT8: Full support with proper calibration
//! - INT4: Limited support, may fall back to GPU

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Quantization precision levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationPrecision {
    /// 32-bit floating point (no quantization)
    Float32,
    /// 16-bit floating point (2x compression)
    Float16,
    /// 8-bit integer (4x compression)
    Int8,
    /// 4-bit integer (8x compression, experimental)
    Int4,
}

impl QuantizationPrecision {
    /// Get bits per value
    pub fn bits(&self) -> u8 {
        match self {
            Self::Float32 => 32,
            Self::Float16 => 16,
            Self::Int8 => 8,
            Self::Int4 => 4,
        }
    }

    /// Get memory compression ratio vs FP32
    pub fn compression_ratio(&self) -> f32 {
        32.0 / self.bits() as f32
    }

    /// Check ANE compatibility
    pub fn is_ane_compatible(&self) -> bool {
        matches!(self, Self::Float16 | Self::Int8)
    }

    /// Get recommended calibration samples
    pub fn recommended_calibration_samples(&self) -> usize {
        match self {
            Self::Float32 | Self::Float16 => 0, // No calibration needed
            Self::Int8 => 512,
            Self::Int4 => 1024,
        }
    }
}

/// Quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    /// Target precision
    pub precision: QuantizationPrecision,
    /// Use symmetric quantization (recommended for ANE)
    pub symmetric: bool,
    /// Per-channel vs per-tensor quantization
    pub per_channel: bool,
    /// Calibration configuration
    pub calibration: Option<CalibrationConfig>,
    /// Quantization mode (static vs dynamic)
    pub mode: QuantizationMode,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            precision: QuantizationPrecision::Float16,
            symmetric: true,
            per_channel: true,
            calibration: None,
            mode: QuantizationMode::Static,
        }
    }
}

/// Quantization mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationMode {
    /// Static quantization (weights + activations)
    Static,
    /// Dynamic quantization (weights only, activations in FP32)
    Dynamic,
}

/// Calibration configuration for INT8/INT4 quantization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationConfig {
    /// Number of calibration samples
    pub num_samples: usize,
    /// Calibration method
    pub method: CalibrationMethod,
    /// Target accuracy threshold (0.0-1.0)
    pub accuracy_threshold: f32,
    /// Maximum calibration steps
    pub max_steps: usize,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            num_samples: 512,
            method: CalibrationMethod::MinMax,
            accuracy_threshold: 0.99,
            max_steps: 1000,
        }
    }
}

/// Calibration method for quantization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibrationMethod {
    /// Min-max calibration (simple, fast)
    MinMax,
    /// Percentile-based calibration (robust to outliers)
    Percentile,
    /// Entropy calibration (optimal for accuracy)
    Entropy,
    /// Mean Squared Error minimization
    MSE,
}

/// Quantization statistics for a tensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationStats {
    /// Minimum value
    pub min: f32,
    /// Maximum value
    pub max: f32,
    /// Mean value
    pub mean: f32,
    /// Standard deviation
    pub std_dev: f32,
    /// Scale factor (for quantization)
    pub scale: f32,
    /// Zero point (for asymmetric quantization)
    pub zero_point: i32,
}

impl QuantizationStats {
    /// Compute statistics from tensor values
    pub fn compute(values: &[f32], symmetric: bool) -> Self {
        let min = values.iter().copied().fold(f32::INFINITY, f32::min);
        let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean = values.iter().sum::<f32>() / values.len() as f32;

        let variance = values
            .iter()
            .map(|&v| (v - mean).powi(2))
            .sum::<f32>()
            / values.len() as f32;
        let std_dev = variance.sqrt();

        let (scale, zero_point) = if symmetric {
            // Symmetric quantization: scale from max absolute value
            let abs_max = max.abs().max(min.abs());
            let scale = abs_max / 127.0; // INT8 range: -127 to 127
            (scale, 0)
        } else {
            // Asymmetric quantization: scale from min/max range
            let scale = (max - min) / 255.0; // INT8 range: 0 to 255
            let zero_point = (-min / scale).round() as i32;
            (scale, zero_point)
        };

        Self {
            min,
            max,
            mean,
            std_dev,
            scale,
            zero_point,
        }
    }

    /// Quantize a value to INT8
    pub fn quantize_i8(&self, value: f32) -> i8 {
        let quantized = (value / self.scale).round() as i32 + self.zero_point;
        quantized.clamp(-128, 127) as i8
    }

    /// Dequantize an INT8 value back to FP32
    pub fn dequantize_i8(&self, quantized: i8) -> f32 {
        (quantized as i32 - self.zero_point) as f32 * self.scale
    }

    /// Quantize a value to INT4
    pub fn quantize_i4(&self, value: f32) -> i8 {
        let quantized = (value / self.scale).round() as i32 + self.zero_point;
        quantized.clamp(-8, 7) as i8
    }

    /// Dequantize an INT4 value back to FP32
    pub fn dequantize_i4(&self, quantized: i8) -> f32 {
        (quantized as i32 - self.zero_point) as f32 * self.scale
    }
}

/// Quantization engine for tensor quantization
pub struct QuantizationEngine {
    config: QuantizationConfig,
    stats_cache: HashMap<String, QuantizationStats>,
}

impl QuantizationEngine {
    /// Create a new quantization engine
    pub fn new(config: QuantizationConfig) -> Result<Self> {
        // Validate configuration
        if !config.precision.is_ane_compatible() {
            warn!(
                "Quantization precision {:?} may not be ANE-compatible",
                config.precision
            );
        }

        if matches!(config.precision, QuantizationPrecision::Int8 | QuantizationPrecision::Int4)
            && config.calibration.is_none()
        {
            return Err(AosError::Config(
                "INT8/INT4 quantization requires calibration configuration".to_string(),
            ));
        }

        Ok(Self {
            config,
            stats_cache: HashMap::new(),
        })
    }

    /// Quantize a tensor to target precision
    pub fn quantize_tensor(
        &mut self,
        name: &str,
        values: &[f32],
    ) -> Result<QuantizedTensor> {
        match self.config.precision {
            QuantizationPrecision::Float32 => {
                // No quantization
                Ok(QuantizedTensor::Float32(values.to_vec()))
            }
            QuantizationPrecision::Float16 => {
                // FP16 quantization
                let fp16_values = self.quantize_fp16(values)?;
                Ok(QuantizedTensor::Float16(fp16_values))
            }
            QuantizationPrecision::Int8 => {
                // INT8 quantization
                let (quantized, stats) = self.quantize_int8(name, values)?;
                self.stats_cache.insert(name.to_string(), stats.clone());
                Ok(QuantizedTensor::Int8 { data: quantized, stats })
            }
            QuantizationPrecision::Int4 => {
                // INT4 quantization
                let (quantized, stats) = self.quantize_int4(name, values)?;
                self.stats_cache.insert(name.to_string(), stats.clone());
                Ok(QuantizedTensor::Int4 { data: quantized, stats })
            }
        }
    }

    /// Quantize to FP16
    fn quantize_fp16(&self, values: &[f32]) -> Result<Vec<u16>> {
        use half::f16;

        let fp16_values: Vec<u16> = values
            .iter()
            .map(|&v| f16::from_f32(v).to_bits())
            .collect();

        debug!(
            "Quantized {} FP32 values to FP16 ({:.1}% compression)",
            values.len(),
            self.config.precision.compression_ratio() * 100.0 - 100.0
        );

        Ok(fp16_values)
    }

    /// Quantize to INT8
    fn quantize_int8(&self, name: &str, values: &[f32]) -> Result<(Vec<i8>, QuantizationStats)> {
        let stats = QuantizationStats::compute(values, self.config.symmetric);

        let quantized: Vec<i8> = values.iter().map(|&v| stats.quantize_i8(v)).collect();

        debug!(
            "{}: INT8 quantization (scale={:.6}, zero_point={})",
            name, stats.scale, stats.zero_point
        );

        Ok((quantized, stats))
    }

    /// Quantize to INT4
    fn quantize_int4(&self, name: &str, values: &[f32]) -> Result<(Vec<i8>, QuantizationStats)> {
        let stats = QuantizationStats::compute(values, self.config.symmetric);

        let quantized: Vec<i8> = values.iter().map(|&v| stats.quantize_i4(v)).collect();

        debug!(
            "{}: INT4 quantization (scale={:.6}, zero_point={})",
            name, stats.scale, stats.zero_point
        );

        Ok((quantized, stats))
    }

    /// Get quantization statistics for a tensor
    pub fn get_stats(&self, name: &str) -> Option<&QuantizationStats> {
        self.stats_cache.get(name)
    }

    /// Export quantization statistics as JSON
    pub fn export_stats(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.stats_cache).map_err(|e| {
            AosError::Validation(format!("Failed to serialize stats: {}", e))
        })
    }

    /// Import quantization statistics from JSON
    pub fn import_stats(&mut self, json: &str) -> Result<()> {
        let stats: HashMap<String, QuantizationStats> = serde_json::from_str(json).map_err(|e| {
            AosError::Validation(format!("Failed to deserialize stats: {}", e))
        })?;

        self.stats_cache = stats;
        Ok(())
    }
}

/// Quantized tensor data
#[derive(Debug, Clone)]
pub enum QuantizedTensor {
    Float32(Vec<f32>),
    Float16(Vec<u16>),
    Int8 {
        data: Vec<i8>,
        stats: QuantizationStats,
    },
    Int4 {
        data: Vec<i8>,
        stats: QuantizationStats,
    },
}

impl QuantizedTensor {
    /// Get precision of quantized tensor
    pub fn precision(&self) -> QuantizationPrecision {
        match self {
            Self::Float32(_) => QuantizationPrecision::Float32,
            Self::Float16(_) => QuantizationPrecision::Float16,
            Self::Int8 { .. } => QuantizationPrecision::Int8,
            Self::Int4 { .. } => QuantizationPrecision::Int4,
        }
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            Self::Float32(v) => v.len() * 4,
            Self::Float16(v) => v.len() * 2,
            Self::Int8 { data, .. } => data.len(),
            Self::Int4 { data, .. } => (data.len() + 1) / 2, // 2 values per byte
        }
    }

    /// Dequantize back to FP32
    pub fn dequantize(&self) -> Vec<f32> {
        match self {
            Self::Float32(v) => v.clone(),
            Self::Float16(v) => {
                use half::f16;
                v.iter()
                    .map(|&bits| f16::from_bits(bits).to_f32())
                    .collect()
            }
            Self::Int8 { data, stats } => {
                data.iter().map(|&v| stats.dequantize_i8(v)).collect()
            }
            Self::Int4 { data, stats } => {
                data.iter().map(|&v| stats.dequantize_i4(v)).collect()
            }
        }
    }
}

/// Accuracy metrics for quantization validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationAccuracyMetrics {
    /// Mean absolute error
    pub mae: f32,
    /// Mean squared error
    pub mse: f32,
    /// Root mean squared error
    pub rmse: f32,
    /// Maximum absolute error
    pub max_error: f32,
    /// Signal-to-noise ratio (dB)
    pub snr_db: f32,
}

impl QuantizationAccuracyMetrics {
    /// Compute accuracy metrics between original and quantized values
    pub fn compute(original: &[f32], dequantized: &[f32]) -> Self {
        assert_eq!(
            original.len(),
            dequantized.len(),
            "Arrays must have same length"
        );

        let n = original.len() as f32;

        // Mean absolute error
        let mae = original
            .iter()
            .zip(dequantized.iter())
            .map(|(&o, &d)| (o - d).abs())
            .sum::<f32>()
            / n;

        // Mean squared error
        let mse = original
            .iter()
            .zip(dequantized.iter())
            .map(|(&o, &d)| (o - d).powi(2))
            .sum::<f32>()
            / n;

        // Root mean squared error
        let rmse = mse.sqrt();

        // Maximum absolute error
        let max_error = original
            .iter()
            .zip(dequantized.iter())
            .map(|(&o, &d)| (o - d).abs())
            .fold(0.0f32, f32::max);

        // Signal-to-noise ratio
        let signal_power = original.iter().map(|&v| v.powi(2)).sum::<f32>() / n;
        let noise_power = mse;
        let snr_db = 10.0 * (signal_power / noise_power).log10();

        Self {
            mae,
            mse,
            rmse,
            max_error,
            snr_db,
        }
    }

    /// Check if accuracy meets threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        // Threshold based on relative error
        self.mae < threshold && self.snr_db > 20.0 // SNR > 20dB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_precision() {
        assert_eq!(QuantizationPrecision::Float32.bits(), 32);
        assert_eq!(QuantizationPrecision::Float16.bits(), 16);
        assert_eq!(QuantizationPrecision::Int8.bits(), 8);
        assert_eq!(QuantizationPrecision::Int4.bits(), 4);

        assert_eq!(QuantizationPrecision::Float16.compression_ratio(), 2.0);
        assert_eq!(QuantizationPrecision::Int8.compression_ratio(), 4.0);
        assert_eq!(QuantizationPrecision::Int4.compression_ratio(), 8.0);
    }

    #[test]
    fn test_ane_compatibility() {
        assert!(!QuantizationPrecision::Float32.is_ane_compatible());
        assert!(QuantizationPrecision::Float16.is_ane_compatible());
        assert!(QuantizationPrecision::Int8.is_ane_compatible());
        assert!(!QuantizationPrecision::Int4.is_ane_compatible());
    }

    #[test]
    fn test_quantization_stats() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = QuantizationStats::compute(&values, true);

        assert_eq!(stats.mean, 3.0);
        assert!(stats.scale > 0.0);
        assert_eq!(stats.zero_point, 0); // Symmetric
    }

    #[test]
    fn test_quantize_dequantize_i8() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = QuantizationStats::compute(&values, true);

        let quantized: Vec<i8> = values.iter().map(|&v| stats.quantize_i8(v)).collect();
        let dequantized: Vec<f32> = quantized.iter().map(|&v| stats.dequantize_i8(v)).collect();

        // Check accuracy
        for (orig, deq) in values.iter().zip(dequantized.iter()) {
            assert!((orig - deq).abs() < 0.1, "Error too large: {} vs {}", orig, deq);
        }
    }

    #[test]
    fn test_quantization_engine_fp16() {
        let config = QuantizationConfig {
            precision: QuantizationPrecision::Float16,
            ..Default::default()
        };

        let mut engine = QuantizationEngine::new(config).unwrap();

        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let quantized = engine.quantize_tensor("test", &values).unwrap();

        assert_eq!(quantized.precision(), QuantizationPrecision::Float16);
        assert!(quantized.size_bytes() < values.len() * 4);
    }

    #[test]
    fn test_accuracy_metrics() {
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let dequantized = vec![1.05, 1.95, 3.05, 3.95, 5.05];

        let metrics = QuantizationAccuracyMetrics::compute(&original, &dequantized);

        assert!(metrics.mae < 0.1);
        assert!(metrics.rmse < 0.1);
        assert!(metrics.snr_db > 20.0);
    }
}
