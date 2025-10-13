//! Tensor types and metadata structures

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tensor data type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum DataType {
    Float32 = 0,
    Float16 = 1,
    Int8 = 2,
    Int16 = 3,
    Int32 = 4,
    Int64 = 5,
    UInt8 = 6,
    UInt16 = 7,
    UInt32 = 8,
    UInt64 = 9,
    Bool = 10,
}

impl DataType {
    /// Get the size in bytes for this data type
    pub fn size_bytes(&self) -> usize {
        match self {
            DataType::Float32 => 4,
            DataType::Float16 => 2,
            DataType::Int8 => 1,
            DataType::Int16 => 2,
            DataType::Int32 => 4,
            DataType::Int64 => 8,
            DataType::UInt8 => 1,
            DataType::UInt16 => 2,
            DataType::UInt32 => 4,
            DataType::UInt64 => 8,
            DataType::Bool => 1,
        }
    }

    /// Convert from string representation
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "float32" | "f32" => Ok(DataType::Float32),
            "float16" | "f16" => Ok(DataType::Float16),
            "int8" | "i8" => Ok(DataType::Int8),
            "int16" | "i16" => Ok(DataType::Int16),
            "int32" | "i32" => Ok(DataType::Int32),
            "int64" | "i64" => Ok(DataType::Int64),
            "uint8" | "u8" => Ok(DataType::UInt8),
            "uint16" | "u16" => Ok(DataType::UInt16),
            "uint32" | "u32" => Ok(DataType::UInt32),
            "uint64" | "u64" => Ok(DataType::UInt64),
            "bool" => Ok(DataType::Bool),
            _ => Err(AosError::Validation(format!("Unknown data type: {}", s))),
        }
    }

    /// Convert to string representation
    pub fn to_str(&self) -> &'static str {
        match self {
            DataType::Float32 => "float32",
            DataType::Float16 => "float16",
            DataType::Int8 => "int8",
            DataType::Int16 => "int16",
            DataType::Int32 => "int32",
            DataType::Int64 => "int64",
            DataType::UInt8 => "uint8",
            DataType::UInt16 => "uint16",
            DataType::UInt32 => "uint32",
            DataType::UInt64 => "uint64",
            DataType::Bool => "bool",
        }
    }
}

/// Memory layout specification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum MemoryLayout {
    RowMajor = 0,
    ColumnMajor = 1,
    Strided = 2,
}

/// Device family enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum DeviceFamily {
    CPU = 0,
    MetalM1 = 1,
    MetalM2 = 2,
    MetalM3 = 3,
    MetalM4 = 4,
    MetalM3Ultra = 5,
    MetalM4Ultra = 6,
}

/// Quantization parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuantizationParams {
    /// Quantization type
    pub quant_type: String,
    /// Group size for block quantization
    pub group_size: Option<u32>,
    /// Bits per weight
    pub bits: Option<u8>,
    /// Scale factors
    pub scales: Option<Vec<f32>>,
    /// Zero points
    pub zero_points: Option<Vec<i8>>,
    /// Additional parameters
    pub extra_params: HashMap<String, f32>,
}

/// Tensor metadata structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TensorMetadata {
    /// Data type
    pub dtype: DataType,
    /// Shape dimensions
    pub shape: Vec<u64>,
    /// Memory layout
    pub layout: MemoryLayout,
    /// Device family
    pub device_family: DeviceFamily,
    /// Quantization parameters (if applicable)
    pub quantization: Option<QuantizationParams>,
    /// Metal kernel hash (if applicable)
    pub metal_kernel_hash: Option<String>,
    /// Memory address hash (for content addressing)
    pub memory_address_hash: Option<String>,
}

/// Tensor structure with metadata and data
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Tensor metadata
    pub metadata: TensorMetadata,
    /// Raw tensor data
    pub data: Vec<u8>,
}

impl Tensor {
    /// Create a new tensor
    pub fn new(
        dtype: DataType,
        shape: Vec<u64>,
        layout: MemoryLayout,
        device_family: DeviceFamily,
        data: Vec<u8>,
    ) -> Result<Self> {
        // Validate data size matches expected size
        let expected_size = shape.iter().product::<u64>() * dtype.size_bytes() as u64;
        if data.len() as u64 != expected_size {
            return Err(AosError::Validation(format!(
                "Data size mismatch: expected {}, got {}",
                expected_size,
                data.len()
            )));
        }

        Ok(Self {
            metadata: TensorMetadata {
                dtype,
                shape,
                layout,
                device_family,
                quantization: None,
                metal_kernel_hash: None,
                memory_address_hash: None,
            },
            data,
        })
    }

    /// Get tensor metadata
    pub fn metadata(&self) -> &TensorMetadata {
        &self.metadata
    }

    /// Get tensor data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get total number of elements
    pub fn numel(&self) -> u64 {
        self.metadata.shape.iter().product()
    }

    /// Get data type size in bytes
    pub fn dtype_size(&self) -> usize {
        self.metadata.dtype.size_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_type_size() {
        assert_eq!(DataType::Float32.size_bytes(), 4);
        assert_eq!(DataType::Float16.size_bytes(), 2);
        assert_eq!(DataType::Int8.size_bytes(), 1);
        assert_eq!(DataType::Bool.size_bytes(), 1);
    }

    #[test]
    fn test_data_type_conversion() {
        assert_eq!(DataType::from_str("float32").unwrap(), DataType::Float32);
        assert_eq!(DataType::from_str("f16").unwrap(), DataType::Float16);
        assert_eq!(DataType::Float32.to_str(), "float32");
    }

    #[test]
    fn test_tensor_creation() {
        let shape = vec![2, 3];
        let data = vec![0u8; 24]; // 2*3*4 bytes for f32
        let tensor = Tensor::new(
            DataType::Float32,
            shape,
            MemoryLayout::RowMajor,
            DeviceFamily::CPU,
            data,
        );
        assert!(tensor.is_ok());
    }

    #[test]
    fn test_tensor_size_validation() {
        let shape = vec![2, 3];
        let data = vec![0u8; 12]; // Wrong size for f32
        let tensor = Tensor::new(
            DataType::Float32,
            shape,
            MemoryLayout::RowMajor,
            DeviceFamily::CPU,
            data,
        );
        assert!(tensor.is_err());
    }
}
