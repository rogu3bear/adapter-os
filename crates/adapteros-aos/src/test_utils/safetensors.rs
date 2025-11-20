//! Safetensors format builder for test data generation
//!
//! Creates valid safetensors-formatted binary data without requiring the full safetensors crate.
//! This is a minimal implementation for testing purposes only.

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tensor data type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorDtype {
    /// 32-bit floating point
    F32,
    /// 16-bit floating point
    F16,
    /// 32-bit integer
    I32,
    /// 16-bit integer (Q15 quantized)
    I16,
}

impl TensorDtype {
    pub fn as_str(&self) -> &'static str {
        match self {
            TensorDtype::F32 => "F32",
            TensorDtype::F16 => "F16",
            TensorDtype::I32 => "I32",
            TensorDtype::I16 => "I16",
        }
    }

    pub fn size(&self) -> usize {
        match self {
            TensorDtype::F32 => 4,
            TensorDtype::F16 => 2,
            TensorDtype::I32 => 4,
            TensorDtype::I16 => 2,
        }
    }
}

/// Tensor configuration for building safetensors
#[derive(Debug, Clone)]
pub struct TensorConfig {
    pub name: String,
    pub dtype: TensorDtype,
    pub shape: Vec<usize>,
    pub data_offset: usize,
    pub data_size: usize,
}

/// Safetensors metadata entry
#[derive(Serialize, Deserialize, Debug)]
struct TensorMetadata {
    dtype: String,
    shape: Vec<usize>,
    data_offsets: [usize; 2],
}

/// Safetensors file builder
///
/// Creates a valid safetensors binary format:
/// ```text
/// [0-7]     header_size (u64, little-endian)
/// [8..]     header_json (JSON metadata)
/// [offset]  tensor_data (binary data for all tensors)
/// ```
pub struct SafetensorsBuilder {
    tensors: Vec<(String, Vec<u8>, Vec<usize>, TensorDtype)>,
}

impl SafetensorsBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            tensors: Vec::new(),
        }
    }

    /// Add a tensor with f32 data
    pub fn add_tensor(&mut self, name: String, data: Vec<f32>, shape: Vec<usize>) {
        let bytes = data
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect::<Vec<u8>>();
        self.tensors.push((name, bytes, shape, TensorDtype::F32));
    }

    /// Add a tensor with f16 data (as u16 representation)
    pub fn add_tensor_f16(&mut self, name: String, data: Vec<u16>, shape: Vec<usize>) {
        let bytes = data
            .iter()
            .flat_map(|&h| h.to_le_bytes())
            .collect::<Vec<u8>>();
        self.tensors.push((name, bytes, shape, TensorDtype::F16));
    }

    /// Add a tensor with i16 data (Q15 quantized)
    pub fn add_tensor_i16(&mut self, name: String, data: Vec<i16>, shape: Vec<usize>) {
        let bytes = data
            .iter()
            .flat_map(|&i| i.to_le_bytes())
            .collect::<Vec<u8>>();
        self.tensors.push((name, bytes, shape, TensorDtype::I16));
    }

    /// Add raw tensor data
    pub fn add_tensor_raw(
        &mut self,
        name: String,
        data: Vec<u8>,
        shape: Vec<usize>,
        dtype: TensorDtype,
    ) {
        self.tensors.push((name, data, shape, dtype));
    }

    /// Build the safetensors binary format
    pub fn build(self) -> Result<Vec<u8>> {
        // Build metadata JSON
        let mut metadata: HashMap<String, TensorMetadata> = HashMap::new();
        let mut current_offset = 0;

        for (name, data, shape, dtype) in &self.tensors {
            let data_size = data.len();
            metadata.insert(
                name.clone(),
                TensorMetadata {
                    dtype: dtype.as_str().to_string(),
                    shape: shape.clone(),
                    data_offsets: [current_offset, current_offset + data_size],
                },
            );
            current_offset += data_size;
        }

        // Serialize metadata to JSON
        let metadata_json = serde_json::to_string(&metadata)?;
        let metadata_bytes = metadata_json.as_bytes();
        let header_size = metadata_bytes.len() as u64;

        // Build the file
        let mut buffer = Vec::new();

        // Write header size (u64, little-endian)
        buffer.extend_from_slice(&header_size.to_le_bytes());

        // Write metadata JSON
        buffer.extend_from_slice(metadata_bytes);

        // Write tensor data
        for (_, data, _, _) in self.tensors {
            buffer.extend_from_slice(&data);
        }

        Ok(buffer)
    }

    /// Build empty safetensors (no tensors)
    pub fn build_empty() -> Result<Vec<u8>> {
        let builder = SafetensorsBuilder::new();
        builder.build()
    }

    /// Build with minimal data (smallest valid safetensors)
    pub fn build_minimal() -> Result<Vec<u8>> {
        let mut builder = SafetensorsBuilder::new();
        builder.add_tensor("x".to_string(), vec![1.0f32], vec![1]);
        builder.build()
    }
}

impl Default for SafetensorsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: Convert f32 slice to f16 (half-precision) representation
///
/// This is a simplified conversion for testing. For production, use half crate.
pub fn f32_to_f16_simple(values: &[f32]) -> Vec<u16> {
    values
        .iter()
        .map(|&f| {
            // Simplified f16 conversion (good enough for test data)
            let bits = f.to_bits();
            let sign = (bits >> 16) & 0x8000;
            let exp = ((bits >> 23) & 0xff) as i32;
            let mant = bits & 0x7fffff;

            if exp == 0 {
                return sign as u16; // Zero or denormal -> zero
            }
            if exp == 0xff {
                return (sign | 0x7c00) as u16; // Inf or NaN
            }

            // Rebias exponent
            let new_exp = exp - 127 + 15;
            if new_exp <= 0 {
                return sign as u16; // Underflow -> zero
            }
            if new_exp >= 31 {
                return (sign | 0x7c00) as u16; // Overflow -> inf
            }

            // Round mantissa to 10 bits
            let new_mant = (mant + 0x1000) >> 13;

            ((sign as u16) | ((new_exp as u16) << 10) | (new_mant as u16))
        })
        .collect()
}

/// Helper: Convert f32 values to Q15 quantized i16
///
/// Q15 format uses 16-bit signed integers where -32768 = -1.0 and 32767 ≈ 1.0
pub fn f32_to_q15(values: &[f32]) -> Vec<i16> {
    values
        .iter()
        .map(|&f| {
            let clamped = f.max(-1.0).min(1.0);
            (clamped * 32767.0).round() as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_empty_safetensors() -> Result<()> {
        let data = SafetensorsBuilder::build_empty()?;

        // Should have at least 8 bytes for header size
        assert!(data.len() >= 8, "Should have header");

        // Parse header size
        let header_size = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        // Empty metadata should be "{}"
        assert!(header_size >= 2, "Should have at least {{}}");

        Ok(())
    }

    #[test]
    fn test_build_minimal_safetensors() -> Result<()> {
        let data = SafetensorsBuilder::build_minimal()?;

        assert!(data.len() > 8, "Should have header and data");

        let header_size = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        assert!(header_size > 0, "Should have metadata");

        Ok(())
    }

    #[test]
    fn test_add_tensor() -> Result<()> {
        let mut builder = SafetensorsBuilder::new();
        builder.add_tensor("test".to_string(), vec![1.0, 2.0, 3.0], vec![3]);

        let data = builder.build()?;

        assert!(data.len() > 8, "Should have content");

        // Parse header
        let header_size = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        // Verify metadata JSON exists
        let metadata_end = 8 + header_size as usize;
        assert!(metadata_end <= data.len(), "Metadata should fit in file");

        let metadata_json = &data[8..metadata_end];
        let metadata: HashMap<String, TensorMetadata> = serde_json::from_slice(metadata_json)?;

        assert!(metadata.contains_key("test"), "Should have 'test' tensor");

        Ok(())
    }

    #[test]
    fn test_multiple_tensors() -> Result<()> {
        let mut builder = SafetensorsBuilder::new();
        builder.add_tensor("lora_A".to_string(), vec![1.0; 100], vec![10, 10]);
        builder.add_tensor("lora_B".to_string(), vec![2.0; 100], vec![10, 10]);

        let data = builder.build()?;
        assert!(data.len() > 100, "Should have both tensors");

        Ok(())
    }

    #[test]
    fn test_f32_to_q15_conversion() {
        let values = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
        let q15_values = f32_to_q15(&values);

        assert_eq!(q15_values[0], -32767); // -1.0
        assert_eq!(q15_values[2], 0); // 0.0
        assert_eq!(q15_values[4], 32767); // 1.0
    }

    #[test]
    fn test_f32_to_f16_conversion() {
        let values = vec![0.0, 1.0, -1.0, 0.5, -0.5];
        let f16_values = f32_to_f16_simple(&values);

        // Basic validation - just check we got values
        assert_eq!(f16_values.len(), 5);
        assert_eq!(f16_values[0], 0); // 0.0 should be 0
    }

    #[test]
    fn test_tensor_dtype_properties() {
        assert_eq!(TensorDtype::F32.size(), 4);
        assert_eq!(TensorDtype::F16.size(), 2);
        assert_eq!(TensorDtype::I16.size(), 2);
        assert_eq!(TensorDtype::F32.as_str(), "F32");
    }
}
