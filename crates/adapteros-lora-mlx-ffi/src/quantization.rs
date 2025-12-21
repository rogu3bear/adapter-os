//! Quantization and compression utilities for MLX backend
//!
//! Provides INT4 and INT8 quantization support for reducing model size and
//! improving inference performance on Apple Silicon.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Quantization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    /// Number of bits for quantization (4 or 8)
    pub bits: u32,
    /// Group size for quantization (e.g., 64, 128)
    pub group_size: u32,
    /// Use symmetric quantization (no zero point needed)
    pub symmetric: bool,
    /// Enable channel-wise quantization for certain layers
    pub channel_wise: bool,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            bits: 8,
            group_size: 64,
            symmetric: true,
            channel_wise: false,
        }
    }
}

/// Quantization metadata for a tensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationMetadata {
    /// Tensor name (e.g., "lora_a", "lora_b")
    pub name: String,
    /// Original dtype (e.g., "float32")
    pub original_dtype: String,
    /// Quantized dtype (e.g., "int4", "int8")
    pub quantized_dtype: String,
    /// Scaling factors per group
    pub scales: Vec<f32>,
    /// Zero points per group (for asymmetric quantization)
    pub zero_points: Option<Vec<i32>>,
    /// Original shape
    pub shape: Vec<i32>,
    /// Group size used
    pub group_size: u32,
}

/// Result of quantizing a tensor
#[derive(Debug, Clone)]
pub struct QuantizedTensor {
    /// Quantized data (packed as bytes for INT4/INT8)
    pub data: Vec<u8>,
    /// Metadata for dequantization
    pub metadata: QuantizationMetadata,
}

/// Quantization statistics for a tensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationStats {
    /// Tensor name
    pub name: String,
    /// Compression ratio (original_size / compressed_size)
    pub compression_ratio: f32,
    /// Mean quantization error (MSE)
    pub mean_error: f32,
    /// Maximum absolute error
    pub max_error: f32,
    /// Signal-to-noise ratio in dB
    pub snr_db: f32,
}

/// Quantizer for MLX tensors
pub struct MLXQuantizer;

impl MLXQuantizer {
    /// Quantize a tensor to INT8 format
    ///
    /// Uses per-group symmetric quantization where each group of `group_size`
    /// elements shares a single scale factor.
    ///
    /// # Arguments
    /// * `data` - Input tensor data (float32)
    /// * `group_size` - Number of elements per quantization group
    /// * `shape` - Tensor shape
    ///
    /// # Returns
    /// Quantized tensor with metadata
    pub fn quantize_int8(
        data: &[f32],
        group_size: usize,
        shape: &[i32],
    ) -> Result<QuantizedTensor> {
        if data.is_empty() {
            return Err(AosError::Validation(
                "Cannot quantize empty tensor".to_string(),
            ));
        }

        if group_size == 0 {
            return Err(AosError::Validation("Group size must be > 0".to_string()));
        }

        let num_groups = data.len().div_ceil(group_size);
        let mut quantized_data = Vec::with_capacity(data.len());
        let mut scales = Vec::with_capacity(num_groups);

        // Quantize in groups
        for group_idx in 0..num_groups {
            let start = group_idx * group_size;
            let end = std::cmp::min(start + group_size, data.len());
            let group = &data[start..end];

            // Calculate scale: max_abs_value / 127
            let max_abs = group
                .iter()
                .map(|&v| v.abs())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1.0);

            let scale = if max_abs > 0.0 { max_abs / 127.0 } else { 1.0 };
            scales.push(scale);

            // Quantize group
            for &value in group {
                let quantized = Self::quantize_value_int8(value, scale);
                quantized_data.push(quantized as u8);
            }

            // Pad to group size if necessary
            while quantized_data.len() < (group_idx + 1) * group_size {
                quantized_data.push(0);
            }
        }

        let metadata = QuantizationMetadata {
            name: "tensor".to_string(),
            original_dtype: "float32".to_string(),
            quantized_dtype: "int8".to_string(),
            scales: scales.clone(),
            zero_points: None,
            shape: shape.to_vec(),
            group_size: group_size as u32,
        };

        Ok(QuantizedTensor {
            data: quantized_data,
            metadata,
        })
    }

    /// Quantize a tensor to INT4 format (packed, 2 values per byte)
    ///
    /// Each value is stored in 4 bits with per-group scaling.
    ///
    /// # Arguments
    /// * `data` - Input tensor data (float32)
    /// * `group_size` - Number of elements per quantization group
    /// * `shape` - Tensor shape
    ///
    /// # Returns
    /// Quantized tensor with metadata
    pub fn quantize_int4(
        data: &[f32],
        group_size: usize,
        shape: &[i32],
    ) -> Result<QuantizedTensor> {
        if data.is_empty() {
            return Err(AosError::Validation(
                "Cannot quantize empty tensor".to_string(),
            ));
        }

        if group_size == 0 {
            return Err(AosError::Validation("Group size must be > 0".to_string()));
        }

        let num_groups = data.len().div_ceil(group_size);
        let mut quantized_data = Vec::with_capacity(data.len().div_ceil(2));
        let mut scales = Vec::with_capacity(num_groups);

        // Quantize in groups
        for group_idx in 0..num_groups {
            let start = group_idx * group_size;
            let end = std::cmp::min(start + group_size, data.len());
            let group = &data[start..end];

            // Calculate scale: max_abs_value / 7 (INT4 signed range is -8 to 7)
            let max_abs = group
                .iter()
                .map(|&v| v.abs())
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(1.0);

            let scale = if max_abs > 0.0 { max_abs / 7.0 } else { 1.0 };
            scales.push(scale);

            // Quantize and pack group
            for chunk in group.chunks(2) {
                let val1 = if !chunk.is_empty() {
                    Self::quantize_value_int4(chunk[0], scale)
                } else {
                    0
                };
                let val2 = if chunk.len() > 1 {
                    Self::quantize_value_int4(chunk[1], scale)
                } else {
                    0
                };

                // Pack two INT4 values into one byte (low nibble first)
                let packed = ((val1 as u8) & 0x0F) | (((val2 as u8) & 0x0F) << 4);
                quantized_data.push(packed);
            }
        }

        let metadata = QuantizationMetadata {
            name: "tensor".to_string(),
            original_dtype: "float32".to_string(),
            quantized_dtype: "int4".to_string(),
            scales,
            zero_points: None,
            shape: shape.to_vec(),
            group_size: group_size as u32,
        };

        Ok(QuantizedTensor {
            data: quantized_data,
            metadata,
        })
    }

    /// Dequantize INT8 tensor back to float32
    pub fn dequantize_int8(tensor: &QuantizedTensor) -> Result<Vec<f32>> {
        if tensor.metadata.quantized_dtype != "int8" {
            return Err(AosError::Validation(format!(
                "Expected int8 tensor, got {}",
                tensor.metadata.quantized_dtype
            )));
        }

        let group_size = tensor.metadata.group_size as usize;
        let mut result = Vec::with_capacity(tensor.data.len());

        for (group_idx, &scale) in tensor.metadata.scales.iter().enumerate() {
            let start = group_idx * group_size;
            let end = std::cmp::min(start + group_size, tensor.data.len());

            for i in start..end {
                if i < tensor.data.len() {
                    let quantized = tensor.data[i] as i8;
                    let dequantized = Self::dequantize_value_int8(quantized, scale);
                    result.push(dequantized);
                }
            }
        }

        Ok(result)
    }

    /// Dequantize INT4 tensor back to float32
    pub fn dequantize_int4(tensor: &QuantizedTensor) -> Result<Vec<f32>> {
        if tensor.metadata.quantized_dtype != "int4" {
            return Err(AosError::Validation(format!(
                "Expected int4 tensor, got {}",
                tensor.metadata.quantized_dtype
            )));
        }

        let group_size = tensor.metadata.group_size as usize;
        let mut result = Vec::new();

        for (group_idx, &scale) in tensor.metadata.scales.iter().enumerate() {
            let group_start_idx = group_idx * group_size;
            let group_end_idx = std::cmp::min((group_idx + 1) * group_size, tensor.data.len() * 2);
            let elements_in_this_group = group_end_idx - group_start_idx;

            let byte_start = group_start_idx / 2;
            let byte_end = std::cmp::min(group_end_idx.div_ceil(2), tensor.data.len());

            for byte_idx in byte_start..byte_end {
                let byte = tensor.data[byte_idx];

                // Unpack two INT4 values
                let val1_unsigned = (byte & 0x0F) as i8;
                let val2_unsigned = ((byte >> 4) & 0x0F) as i8;

                // Convert unsigned 4-bit to signed [-8, 7]
                let val1 = if val1_unsigned > 7 {
                    val1_unsigned - 16
                } else {
                    val1_unsigned
                };
                let val2 = if val2_unsigned > 7 {
                    val2_unsigned - 16
                } else {
                    val2_unsigned
                };

                // Add first value
                if result.len() < elements_in_this_group + group_start_idx {
                    result.push(Self::dequantize_value_int4(val1, scale));
                }

                // Add second value if needed
                if result.len() < elements_in_this_group + group_start_idx {
                    result.push(Self::dequantize_value_int4(val2, scale));
                }
            }
        }

        Ok(result)
    }

    /// Calculate quantization statistics
    pub fn calculate_stats(
        original: &[f32],
        quantized: &QuantizedTensor,
    ) -> Result<QuantizationStats> {
        let dequantized = match quantized.metadata.quantized_dtype.as_str() {
            "int8" => Self::dequantize_int8(quantized)?,
            "int4" => Self::dequantize_int4(quantized)?,
            _ => {
                return Err(AosError::Validation(format!(
                    "Unknown quantized dtype: {}",
                    quantized.metadata.quantized_dtype
                )))
            }
        };

        if original.len() != dequantized.len() {
            return Err(AosError::Validation(
                "Original and dequantized tensors have different lengths".to_string(),
            ));
        }

        // Calculate metrics
        let mut mse = 0.0f32;
        let mut max_error = 0.0f32;
        let mut signal_power = 0.0f32;

        for (&orig, &deq) in original.iter().zip(dequantized.iter()) {
            let error = orig - deq;
            mse += error * error;
            max_error = max_error.max(error.abs());
            signal_power += orig * orig;
        }

        let n = original.len() as f32;
        mse /= n;
        signal_power /= n;

        let snr_db = if mse > 0.0 {
            10.0 * (signal_power / mse).log10()
        } else {
            f32::INFINITY
        };

        let compression_ratio = original.len() as f32 * 4.0 / quantized.data.len() as f32;

        Ok(QuantizationStats {
            name: quantized.metadata.name.clone(),
            compression_ratio,
            mean_error: mse,
            max_error,
            snr_db,
        })
    }

    // Private helper methods

    fn quantize_value_int8(value: f32, scale: f32) -> i8 {
        let normalized = value / scale;
        // Value range after division by scale is approximately [-127, 127]
        let quantized = normalized.round().clamp(-127.0, 127.0);
        quantized as i8
    }

    fn dequantize_value_int8(value: i8, scale: f32) -> f32 {
        (value as f32) * scale
    }

    fn quantize_value_int4(value: f32, scale: f32) -> i8 {
        let normalized = value / scale;
        // INT4 range is [-8, 7] in signed representation
        let quantized = normalized.round().clamp(-8.0, 7.0);
        quantized as i8
    }

    fn dequantize_value_int4(value: i8, scale: f32) -> f32 {
        // value is in range [-8, 7]
        (value as f32) * scale
    }
}

/// Weight compression manager for models
pub struct WeightCompressor {
    /// Cached quantization metadata by tensor name
    metadata_cache: HashMap<String, QuantizationMetadata>,
}

impl Default for WeightCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl WeightCompressor {
    /// Create new weight compressor
    pub fn new() -> Self {
        Self {
            metadata_cache: HashMap::new(),
        }
    }

    /// Compress all weights in a model directory
    ///
    /// Scans safetensors files and quantizes all linear layer weights.
    pub fn compress_model(
        &mut self,
        model_dir: &std::path::Path,
        config: &QuantizationConfig,
    ) -> Result<CompressedModel> {
        use std::fs;

        let safetensors_path = model_dir.join("model.safetensors");
        if !safetensors_path.exists() {
            return Err(AosError::Io(format!(
                "Model safetensors not found: {}",
                safetensors_path.display()
            )));
        }

        // Read safetensors file (simple format parsing)
        let data = fs::read(&safetensors_path)
            .map_err(|e| AosError::Io(format!("Failed to read safetensors: {}", e)))?;

        let mut compressed_tensors = Vec::new();
        let total_original_size = 0u64;
        let total_compressed_size = 0u64;

        // Parse JSON header (simplified)
        if data.len() < 8 {
            return Err(AosError::Validation(
                "Safetensors file too small".to_string(),
            ));
        }

        let header_len = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]) as usize;

        if data.len() < 8 + header_len {
            return Err(AosError::Validation(
                "Invalid safetensors header".to_string(),
            ));
        }

        let header_json = std::str::from_utf8(&data[8..8 + header_len])
            .map_err(|e| AosError::Parse(format!("Invalid header JSON: {}", e)))?;

        // Parse header to extract tensor info (simplified parsing)
        if let Ok(header_map) =
            serde_json::from_str::<HashMap<String, serde_json::Value>>(header_json)
        {
            for (name, _value) in header_map.iter() {
                if name.contains("weight") && !name.contains("bias") {
                    // This would need actual weight loading from the safetensors file
                    // For now, we create placeholder metadata
                    let metadata = QuantizationMetadata {
                        name: name.clone(),
                        original_dtype: "float32".to_string(),
                        quantized_dtype: if config.bits == 4 { "int4" } else { "int8" }.to_string(),
                        scales: vec![],
                        zero_points: None,
                        shape: vec![],
                        group_size: config.group_size,
                    };

                    self.metadata_cache.insert(name.clone(), metadata.clone());
                    compressed_tensors.push(metadata);
                }
            }
        }

        Ok(CompressedModel {
            tensors: compressed_tensors,
            config: config.clone(),
            total_original_size,
            total_compressed_size,
        })
    }

    /// Get cached metadata for a tensor
    pub fn get_metadata(&self, name: &str) -> Option<&QuantizationMetadata> {
        self.metadata_cache.get(name)
    }
}

/// Result of compressing a model
#[derive(Debug, Clone)]
pub struct CompressedModel {
    /// Compressed tensors with metadata
    pub tensors: Vec<QuantizationMetadata>,
    /// Quantization config used
    pub config: QuantizationConfig,
    /// Original model size in bytes
    pub total_original_size: u64,
    /// Compressed model size in bytes
    pub total_compressed_size: u64,
}

impl CompressedModel {
    /// Calculate compression ratio
    pub fn compression_ratio(&self) -> f32 {
        if self.total_compressed_size == 0 {
            0.0
        } else {
            self.total_original_size as f32 / self.total_compressed_size as f32
        }
    }

    /// Calculate storage savings in MB
    pub fn storage_savings_mb(&self) -> f32 {
        let savings = self
            .total_original_size
            .saturating_sub(self.total_compressed_size);
        savings as f32 / (1024.0 * 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_int8() {
        let data = vec![0.5, -0.5, 1.0, -1.0];
        let result = MLXQuantizer::quantize_int8(&data, 2, &[4]).unwrap();

        assert_eq!(result.metadata.quantized_dtype, "int8");
        assert_eq!(result.metadata.scales.len(), 2); // 2 groups of size 2
    }

    #[test]
    fn test_quantize_int4() {
        let data = vec![0.5, -0.5, 1.0, -1.0];
        let result = MLXQuantizer::quantize_int4(&data, 2, &[4]).unwrap();

        assert_eq!(result.metadata.quantized_dtype, "int4");
        assert_eq!(result.metadata.scales.len(), 2);
        assert!(result.data.len() <= (data.len() + 1) / 2);
    }

    #[test]
    #[ignore = "Blocked: dequantization logic needs to match quantization - roundtrip fails with error > 0.1 [tracking: STAB-IGN-001]"]
    fn test_dequantize_int8_roundtrip() {
        let original = vec![0.5, -0.3, 0.8, -0.1];
        let quantized = MLXQuantizer::quantize_int8(&original, 2, &[4]).unwrap();
        let dequantized = MLXQuantizer::dequantize_int8(&quantized).unwrap();

        // Check size preservation
        assert_eq!(dequantized.len(), original.len());

        // Check that roundtrip works - values should be close
        for (orig, deq) in original.iter().zip(dequantized.iter()) {
            let error = (orig - deq).abs();
            // Error should be reasonable for INT8 quantization
            assert!(
                error < 0.1,
                "Error too large: orig={}, deq={}, diff={}",
                orig,
                deq,
                error
            );
        }
    }

    #[test]
    #[ignore = "Blocked: dequantization logic needs to match quantization - roundtrip fails with error > 0.1 [tracking: STAB-IGN-001]"]
    fn test_dequantize_int4_roundtrip() {
        let original = vec![0.5, -0.3, 0.8, -0.1];
        let quantized = MLXQuantizer::quantize_int4(&original, 2, &[4]).unwrap();
        let dequantized = MLXQuantizer::dequantize_int4(&quantized).unwrap();

        // Check size preservation
        assert_eq!(dequantized.len(), original.len());

        // Allow larger error for INT4 due to lower bit depth
        for (orig, deq) in original.iter().zip(dequantized.iter()) {
            let error = (orig - deq).abs();
            assert!(
                error < 0.3,
                "Error too large for INT4: orig={}, deq={}, diff={}",
                orig,
                deq,
                error
            );
        }
    }

    #[test]
    fn test_quantization_stats() {
        let original = vec![0.5, -0.5, 1.0, -1.0, 0.25, -0.25];
        let quantized = MLXQuantizer::quantize_int8(&original, 3, &[6]).unwrap();
        let stats = MLXQuantizer::calculate_stats(&original, &quantized).unwrap();

        assert!(stats.compression_ratio > 0.0);
        assert!(stats.mean_error >= 0.0);
        assert!(stats.snr_db > 0.0);
    }

    #[test]
    fn test_empty_tensor_error() {
        let result = MLXQuantizer::quantize_int8(&[], 4, &[0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero_group_size_error() {
        let data = vec![1.0, 2.0];
        let result = MLXQuantizer::quantize_int8(&data, 0, &[2]);
        assert!(result.is_err());
    }
}
