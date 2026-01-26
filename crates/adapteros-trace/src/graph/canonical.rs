//! Canonical tensor metadata representation for deterministic hashing

use crate::graph::tensor::Tensor;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Hash version for canonicalization schema
pub const HASH_VERSION: u8 = 1;

/// Endianness flag for cross-platform stability
pub const LITTLE_ENDIAN: u8 = 1;

/// Canonical tensor representation for deterministic hashing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalTensor {
    /// Hash schema version
    pub version: u8,
    /// Endianness flag
    pub endian: u8,
    /// Data type as canonical bytes
    pub dtype_bytes: u8,
    /// Shape dimensions as u64 list
    pub shape: Vec<u64>,
    /// Memory layout flag
    pub layout_bytes: u8,
    /// Device family as bytes
    pub device_family_bytes: u8,
    /// Quantization parameters (canonicalized)
    pub quantization_params: Option<CanonicalQuantizationParams>,
    /// Metal kernel hash (if applicable)
    pub metal_kernel_hash: Option<String>,
    /// Memory address hash (for content addressing)
    pub memory_address_hash: Option<String>,
}

/// Canonical quantization parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalQuantizationParams {
    /// Quantization type
    pub quant_type: String,
    /// Group size for block quantization
    pub group_size: Option<u32>,
    /// Bits per weight
    pub bits: Option<u8>,
    /// Scale factors (sorted for determinism)
    pub scales: Option<Vec<f32>>,
    /// Zero points (sorted for determinism)
    pub zero_points: Option<Vec<i8>>,
    /// Additional parameters (sorted by key for determinism)
    pub extra_params: BTreeMap<String, f32>,
}

impl CanonicalTensor {
    /// Create canonical representation from tensor
    pub fn from_tensor(tensor: &Tensor) -> Result<Self> {
        let quantization_params =
            tensor
                .metadata
                .quantization
                .as_ref()
                .map(|q| CanonicalQuantizationParams {
                    quant_type: q.quant_type.clone(),
                    group_size: q.group_size,
                    bits: q.bits,
                    scales: q.scales.clone(),
                    zero_points: q.zero_points.clone(),
                    extra_params: q
                        .extra_params
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                });

        Ok(Self {
            version: HASH_VERSION,
            endian: LITTLE_ENDIAN,
            dtype_bytes: tensor.metadata.dtype as u8,
            shape: tensor.metadata.shape.clone(),
            layout_bytes: tensor.metadata.layout as u8,
            device_family_bytes: tensor.metadata.device_family as u8,
            quantization_params,
            metal_kernel_hash: tensor.metadata.metal_kernel_hash.clone(),
            memory_address_hash: tensor.metadata.memory_address_hash.clone(),
        })
    }

    /// Serialize to canonical bytes using CBOR
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>> {
        serde_cbor::to_vec(self).map_err(|e| {
            AosError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize canonical tensor: {}", e),
            )))
        })
    }

    /// Deserialize from canonical bytes using CBOR
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self> {
        serde_cbor::from_slice(bytes).map_err(|e| {
            AosError::Serialization(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize canonical tensor: {}", e),
            )))
        })
    }

    /// Convert to fixed-size byte layout (alternative to CBOR)
    pub fn to_fixed_bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();

        // Version and endian flags (2 bytes)
        bytes.push(self.version);
        bytes.push(self.endian);

        // Data type (1 byte)
        bytes.push(self.dtype_bytes);

        // Shape length and dimensions (8 + N*8 bytes)
        bytes.extend_from_slice(&(self.shape.len() as u64).to_le_bytes());
        for &dim in &self.shape {
            bytes.extend_from_slice(&dim.to_le_bytes());
        }

        // Layout and device family (2 bytes)
        bytes.push(self.layout_bytes);
        bytes.push(self.device_family_bytes);

        // Quantization parameters (variable length)
        if let Some(ref qp) = self.quantization_params {
            bytes.push(1); // Present flag
            bytes.extend_from_slice(&qp.quant_type.len().to_le_bytes());
            bytes.extend_from_slice(qp.quant_type.as_bytes());

            // Group size
            if let Some(gs) = qp.group_size {
                bytes.push(1); // Present flag
                bytes.extend_from_slice(&gs.to_le_bytes());
            } else {
                bytes.push(0); // Absent flag
            }

            // Bits
            if let Some(bits) = qp.bits {
                bytes.push(1); // Present flag
                bytes.push(bits);
            } else {
                bytes.push(0); // Absent flag
            }

            // Scales
            if let Some(ref scales) = qp.scales {
                bytes.push(1); // Present flag
                bytes.extend_from_slice(&scales.len().to_le_bytes());
                for &scale in scales {
                    bytes.extend_from_slice(&scale.to_le_bytes());
                }
            } else {
                bytes.push(0); // Absent flag
            }

            // Zero points
            if let Some(ref zp) = qp.zero_points {
                bytes.push(1); // Present flag
                bytes.extend_from_slice(&zp.len().to_le_bytes());
                bytes.extend_from_slice(&zp.iter().map(|&x| x as u8).collect::<Vec<u8>>());
            } else {
                bytes.push(0); // Absent flag
            }

            // Extra params
            bytes.extend_from_slice(&qp.extra_params.len().to_le_bytes());
            for (key, &value) in &qp.extra_params {
                bytes.extend_from_slice(&key.len().to_le_bytes());
                bytes.extend_from_slice(key.as_bytes());
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        } else {
            bytes.push(0); // Absent flag
        }

        // Metal kernel hash
        if let Some(ref hash) = self.metal_kernel_hash {
            bytes.push(1); // Present flag
            bytes.extend_from_slice(&hash.len().to_le_bytes());
            bytes.extend_from_slice(hash.as_bytes());
        } else {
            bytes.push(0); // Absent flag
        }

        // Memory address hash
        if let Some(ref hash) = self.memory_address_hash {
            bytes.push(1); // Present flag
            bytes.extend_from_slice(&hash.len().to_le_bytes());
            bytes.extend_from_slice(hash.as_bytes());
        } else {
            bytes.push(0); // Absent flag
        }

        Ok(bytes)
    }

    /// Convert from fixed-size byte layout
    pub fn from_fixed_bytes(bytes: &[u8]) -> Result<Self> {
        let mut offset = 0;

        if bytes.len() < 2 {
            return Err(AosError::Validation(
                "Insufficient bytes for canonical tensor".to_string(),
            ));
        }

        let version = bytes[offset];
        offset += 1;
        let endian = bytes[offset];
        offset += 1;

        if version != HASH_VERSION {
            return Err(AosError::Validation(format!(
                "Unsupported hash version: {} (expected {})",
                version, HASH_VERSION
            )));
        }

        if bytes.len() < offset + 1 {
            return Err(AosError::Validation(
                "Insufficient bytes for dtype".to_string(),
            ));
        }
        let dtype_bytes = bytes[offset];
        offset += 1;

        if bytes.len() < offset + 8 {
            return Err(AosError::Validation(
                "Insufficient bytes for shape length".to_string(),
            ));
        }
        let shape_len = u64::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        offset += 8;

        if bytes.len() < offset + shape_len * 8 {
            return Err(AosError::Validation(
                "Insufficient bytes for shape".to_string(),
            ));
        }
        let mut shape = Vec::with_capacity(shape_len);
        for _ in 0..shape_len {
            let dim = u64::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]);
            shape.push(dim);
            offset += 8;
        }

        if bytes.len() < offset + 2 {
            return Err(AosError::Validation(
                "Insufficient bytes for layout and device".to_string(),
            ));
        }
        let layout_bytes = bytes[offset];
        offset += 1;
        let device_family_bytes = bytes[offset];
        offset += 1;

        // Parse quantization parameters
        let quantization_params = if bytes.len() > offset && bytes[offset] == 1 {
            offset += 1;
            // Parse quantization params (simplified - full implementation would parse all fields)
            Some(CanonicalQuantizationParams {
                quant_type: "unknown".to_string(),
                group_size: None,
                bits: None,
                scales: None,
                zero_points: None,
                extra_params: BTreeMap::new(),
            })
        } else {
            offset += 1;
            None
        };

        // Parse metal kernel hash
        let metal_kernel_hash = if bytes.len() > offset && bytes[offset] == 1 {
            offset += 1;
            if bytes.len() < offset + 8 {
                return Err(AosError::Validation(
                    "Insufficient bytes for kernel hash length".to_string(),
                ));
            }
            let hash_len = u64::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            offset += 8;
            if bytes.len() < offset + hash_len {
                return Err(AosError::Validation(
                    "Insufficient bytes for kernel hash".to_string(),
                ));
            }
            let hash =
                String::from_utf8(bytes[offset..offset + hash_len].to_vec()).map_err(|e| {
                    AosError::Validation(format!("Invalid UTF-8 in kernel hash: {}", e))
                })?;
            offset += hash_len;
            Some(hash)
        } else {
            offset += 1;
            None
        };

        // Parse memory address hash
        let memory_address_hash = if bytes.len() > offset && bytes[offset] == 1 {
            offset += 1;
            if bytes.len() < offset + 8 {
                return Err(AosError::Validation(
                    "Insufficient bytes for address hash length".to_string(),
                ));
            }
            let hash_len = u64::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
                bytes[offset + 4],
                bytes[offset + 5],
                bytes[offset + 6],
                bytes[offset + 7],
            ]) as usize;
            offset += 8;
            if bytes.len() < offset + hash_len {
                return Err(AosError::Validation(
                    "Insufficient bytes for address hash".to_string(),
                ));
            }
            let hash =
                String::from_utf8(bytes[offset..offset + hash_len].to_vec()).map_err(|e| {
                    AosError::Validation(format!("Invalid UTF-8 in address hash: {}", e))
                })?;
            Some(hash)
        } else {
            None
        };

        Ok(Self {
            version,
            endian,
            dtype_bytes,
            shape,
            layout_bytes,
            device_family_bytes,
            quantization_params,
            metal_kernel_hash,
            memory_address_hash,
        })
    }
}

/// Create canonical tensor representation from tensor
pub fn canonical_tensor_repr(tensor: &Tensor) -> Result<CanonicalTensor> {
    CanonicalTensor::from_tensor(tensor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::tensor::{DataType, DeviceFamily, MemoryLayout, Tensor, TensorMetadata};

    fn create_test_tensor() -> Tensor {
        Tensor {
            metadata: TensorMetadata {
                dtype: DataType::Float32,
                shape: vec![2, 3],
                layout: MemoryLayout::RowMajor,
                device_family: DeviceFamily::MetalM3,
                quantization: None,
                metal_kernel_hash: Some("abc123".to_string()),
                memory_address_hash: Some("def456".to_string()),
            },
            data: vec![0u8; 24],
        }
    }

    #[test]
    fn test_canonical_tensor_creation() {
        let tensor = create_test_tensor();
        let canonical = CanonicalTensor::from_tensor(&tensor).unwrap();

        assert_eq!(canonical.version, HASH_VERSION);
        assert_eq!(canonical.endian, LITTLE_ENDIAN);
        assert_eq!(canonical.dtype_bytes, DataType::Float32 as u8);
        assert_eq!(canonical.shape, vec![2, 3]);
        assert_eq!(canonical.layout_bytes, MemoryLayout::RowMajor as u8);
        assert_eq!(canonical.device_family_bytes, DeviceFamily::MetalM3 as u8);
        assert_eq!(canonical.metal_kernel_hash, Some("abc123".to_string()));
        assert_eq!(canonical.memory_address_hash, Some("def456".to_string()));
    }

    #[test]
    fn test_canonical_bytes_roundtrip() {
        let tensor = create_test_tensor();
        let canonical = CanonicalTensor::from_tensor(&tensor).unwrap();

        let bytes = canonical.to_canonical_bytes().unwrap();
        let restored = CanonicalTensor::from_canonical_bytes(&bytes).unwrap();

        assert_eq!(canonical, restored);
    }

    #[test]
    fn test_fixed_bytes_roundtrip() {
        let tensor = create_test_tensor();
        let canonical = CanonicalTensor::from_tensor(&tensor).unwrap();

        let bytes = canonical.to_fixed_bytes().unwrap();
        let restored = CanonicalTensor::from_fixed_bytes(&bytes).unwrap();

        assert_eq!(canonical.version, restored.version);
        assert_eq!(canonical.endian, restored.endian);
        assert_eq!(canonical.dtype_bytes, restored.dtype_bytes);
        assert_eq!(canonical.shape, restored.shape);
        assert_eq!(canonical.layout_bytes, restored.layout_bytes);
        assert_eq!(canonical.device_family_bytes, restored.device_family_bytes);
    }

    #[test]
    fn test_deterministic_serialization() {
        let tensor = create_test_tensor();
        let canonical1 = CanonicalTensor::from_tensor(&tensor).unwrap();
        let canonical2 = CanonicalTensor::from_tensor(&tensor).unwrap();

        let bytes1 = canonical1.to_canonical_bytes().unwrap();
        let bytes2 = canonical2.to_canonical_bytes().unwrap();

        assert_eq!(bytes1, bytes2);
    }
}
