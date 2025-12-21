//! Quantization API for AdapterOS
//!
//! This module was merged from adapteros-lora-quant crate.

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Quantization specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantSpec {
    /// Quantization type (e.g., "int4_block", "fp16")
    pub quant_type: String,

    /// Group size for block quantization
    pub group_size: Option<u32>,

    /// Bits per weight
    pub bits: Option<u8>,

    /// Quantization algorithm
    pub algorithm: String,

    /// Quantization metadata hash
    pub spec_hash: B3Hash,
}

/// Quantized tensor metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedTensor {
    /// Tensor name
    pub name: String,

    /// Original shape
    pub shape: Vec<u32>,

    /// Quantized data hash
    pub data_hash: B3Hash,

    /// Quantization parameters
    pub quant_params: HashMap<String, f32>,

    /// Scale factors (for block quantization)
    pub scales: Option<Vec<f32>>,

    /// Zero points (for block quantization)
    pub zero_points: Option<Vec<i8>>,
}

/// Quantizer interface
pub trait Quantizer {
    /// Quantize a tensor
    fn quantize_tensor(
        &self,
        name: &str,
        data: &[f32],
        shape: &[u32],
        spec: &QuantSpec,
    ) -> Result<QuantizedTensor>;

    /// Dequantize a tensor
    fn dequantize_tensor(&self, quantized: &QuantizedTensor, spec: &QuantSpec) -> Result<Vec<f32>>;

    /// Get quantization specification
    fn get_spec(&self) -> &QuantSpec;
}

/// Block quantizer implementation
pub struct BlockQuantizer {
    spec: QuantSpec,
}

impl BlockQuantizer {
    /// Create a new block quantizer
    pub fn new(quant_type: String, group_size: u32, bits: u8) -> Self {
        let spec = QuantSpec {
            quant_type: quant_type.clone(),
            group_size: Some(group_size),
            bits: Some(bits),
            algorithm: "block".to_string(),
            spec_hash: B3Hash::hash(format!("{}:{}:{}", quant_type, group_size, bits).as_bytes()),
        };

        Self { spec }
    }

    /// Quantize using block quantization
    fn quantize_block(
        &self,
        data: &[f32],
        group_size: u32,
        bits: u8,
    ) -> Result<(Vec<f32>, Vec<i8>)> {
        let mut scales = Vec::new();
        let mut zero_points = Vec::new();

        // Simple block quantization implementation
        for chunk in data.chunks(group_size as usize) {
            let min_val = chunk.iter().fold(f32::INFINITY, |a, &b| a.min(b));
            let max_val = chunk.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));

            let scale = (max_val - min_val) / ((1 << bits) - 1) as f32;
            let zero_point = (-min_val / scale).round() as i8;

            scales.push(scale);
            zero_points.push(zero_point);
        }

        Ok((scales, zero_points))
    }
}

impl Quantizer for BlockQuantizer {
    fn quantize_tensor(
        &self,
        name: &str,
        data: &[f32],
        shape: &[u32],
        spec: &QuantSpec,
    ) -> Result<QuantizedTensor> {
        let group_size = spec.group_size.unwrap_or(128);
        let bits = spec.bits.unwrap_or(4);

        let (scales, zero_points) = self.quantize_block(data, group_size, bits)?;

        // Create quantized data (simplified - in reality would pack bits)
        let quantized_data = data
            .iter()
            .enumerate()
            .map(|(i, &val)| {
                let group_idx = i / group_size as usize;
                let scale = scales[group_idx];
                let zero_point = zero_points[group_idx];
                ((val / scale + zero_point as f32).round() as i8)
                    .max(0)
                    .min((1 << bits) - 1) as f32
            })
            .collect::<Vec<_>>();

        let data_bytes: Vec<u8> = quantized_data
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();
        let data_hash = B3Hash::hash(&data_bytes);

        let mut quant_params = HashMap::new();
        quant_params.insert("group_size".to_string(), group_size as f32);
        quant_params.insert("bits".to_string(), bits as f32);

        Ok(QuantizedTensor {
            name: name.to_string(),
            shape: shape.to_vec(),
            data_hash,
            quant_params,
            scales: Some(scales),
            zero_points: Some(zero_points),
        })
    }

    fn dequantize_tensor(&self, quantized: &QuantizedTensor, spec: &QuantSpec) -> Result<Vec<f32>> {
        let scales = quantized
            .scales
            .as_ref()
            .ok_or_else(|| AosError::Quantization("Missing scales".to_string()))?;
        let zero_points = quantized
            .zero_points
            .as_ref()
            .ok_or_else(|| AosError::Quantization("Missing zero points".to_string()))?;

        let group_size = spec.group_size.unwrap_or(128) as usize;
        let total_elements: usize = quantized.shape.iter().product::<u32>() as usize;

        let mut dequantized = Vec::with_capacity(total_elements);

        for i in 0..total_elements {
            let group_idx = i / group_size;
            let scale = scales[group_idx];
            let zero_point = zero_points[group_idx] as f32;

            // Simplified dequantization (in reality would unpack bits)
            let quantized_val = 0.0f32; // Placeholder
            let dequantized_val = (quantized_val - zero_point) * scale;
            dequantized.push(dequantized_val);
        }

        Ok(dequantized)
    }

    fn get_spec(&self) -> &QuantSpec {
        &self.spec
    }
}

/// Quantization error type
#[derive(Debug, thiserror::Error)]
pub enum QuantizationError {
    #[error("Invalid quantization parameters: {0}")]
    InvalidParams(String),

    #[error("Quantization failed: {0}")]
    QuantizationFailed(String),

    #[error("Dequantization failed: {0}")]
    DequantizationFailed(String),
}

impl From<QuantizationError> for AosError {
    fn from(err: QuantizationError) -> Self {
        AosError::Quantization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_quantizer() {
        let quantizer = BlockQuantizer::new("int4_block".to_string(), 128, 4);
        let spec = quantizer.get_spec();

        assert_eq!(spec.quant_type, "int4_block");
        assert_eq!(spec.group_size, Some(128));
        assert_eq!(spec.bits, Some(4));
        assert_eq!(spec.algorithm, "block");
    }

    #[test]
    fn test_quantize_tensor() {
        let quantizer = BlockQuantizer::new("int4_block".to_string(), 128, 4);
        let spec = quantizer.get_spec().clone();

        // Test data
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let result = quantizer.quantize_tensor("test", &data, &shape, &spec);
        assert!(result.is_ok());

        let quantized = result.expect("Test quantization should succeed");
        assert_eq!(quantized.name, "test");
        assert_eq!(quantized.shape, shape);
        assert!(quantized.scales.is_some());
        assert!(quantized.zero_points.is_some());
    }
}
