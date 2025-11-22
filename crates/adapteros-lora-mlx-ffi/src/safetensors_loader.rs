//! Safetensors format loader with quantization support
//!
//! Loads weights from safetensors files and supports loading both full-precision
//! and quantized (INT4, INT8) model weights.

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Information about a tensor in safetensors file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorInfo {
    /// Tensor name (e.g., "model.layers.0.self_attn.q_proj.weight")
    pub name: String,
    /// Data type (e.g., "float32", "int4", "int8")
    pub dtype: String,
    /// Tensor shape
    pub shape: Vec<i32>,
    /// Byte offset in file
    pub byte_offset: u64,
    /// Data length in bytes
    pub data_len: u64,
}

/// Safetensors header metadata (parsed from file)
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SafetensorsHeader {
    /// Mapping of tensor name to info
    pub tensors: HashMap<String, TensorInfo>,
    /// Header size in bytes
    pub header_size: u64,
}

/// Loader for safetensors format files
pub struct SafetensorsLoader {
    /// Parsed tensor metadata
    tensors: HashMap<String, TensorInfo>,
    /// File path
    file_path: std::path::PathBuf,
}

impl SafetensorsLoader {
    /// Load a safetensors file and parse its header
    ///
    /// Safetensors format:
    /// - [0-7]: Header size (u64 LE)
    /// - [8..]: JSON header
    /// - [8+header_size..]: Binary tensor data
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file_data = std::fs::read(path)
            .map_err(|e| AosError::Io(format!("Failed to read safetensors file: {}", e)))?;

        if file_data.len() < 8 {
            return Err(AosError::Parse(
                "Safetensors file too small (< 8 bytes)".to_string(),
            ));
        }

        // Parse header size (little-endian u64)
        let header_size = u64::from_le_bytes([
            file_data[0],
            file_data[1],
            file_data[2],
            file_data[3],
            file_data[4],
            file_data[5],
            file_data[6],
            file_data[7],
        ]) as usize;

        if file_data.len() < 8 + header_size {
            return Err(AosError::Parse(
                "Safetensors file corrupted - header extends beyond file".to_string(),
            ));
        }

        // Parse JSON header
        let header_json = std::str::from_utf8(&file_data[8..8 + header_size])
            .map_err(|e| AosError::Parse(format!("Invalid header JSON UTF-8: {}", e)))?;

        let header_map: HashMap<String, serde_json::Value> = serde_json::from_str(header_json)
            .map_err(|e| AosError::Parse(format!("Failed to parse header JSON: {}", e)))?;

        let mut tensors = HashMap::new();
        let mut current_offset = (8 + header_size) as u64;

        // Extract tensor information from header
        for (name, value) in header_map.iter() {
            if name == "__metadata__" {
                continue; // Skip metadata entries
            }

            if let Ok(tensor_desc) = serde_json::from_value::<TensorDescription>(value.clone()) {
                let shape = tensor_desc.shape.clone();
                let dtype = tensor_desc.dtype.clone();
                let data_len = Self::compute_data_length(&shape, &dtype)?;

                let tensor_info = TensorInfo {
                    name: name.clone(),
                    dtype,
                    shape,
                    byte_offset: current_offset,
                    data_len,
                };

                current_offset += data_len;
                tensors.insert(name.clone(), tensor_info);
            }
        }

        tracing::info!(
            path = %path.display(),
            num_tensors = tensors.len(),
            file_size_mb = file_data.len() as f32 / (1024.0 * 1024.0),
            "Safetensors file loaded"
        );

        Ok(Self {
            tensors,
            file_path: path.to_path_buf(),
        })
    }

    /// Get information about a tensor
    pub fn get_tensor_info(&self, name: &str) -> Option<&TensorInfo> {
        self.tensors.get(name)
    }

    /// List all tensor names
    pub fn tensor_names(&self) -> Vec<String> {
        self.tensors.keys().cloned().collect()
    }

    /// Load a specific tensor's data
    ///
    /// Returns the raw bytes of the tensor data
    pub fn load_tensor_data(&self, name: &str) -> Result<Vec<u8>> {
        let info = self
            .get_tensor_info(name)
            .ok_or_else(|| AosError::NotFound(format!("Tensor not found: {}", name)))?;

        let file_data = std::fs::read(&self.file_path)
            .map_err(|e| AosError::Io(format!("Failed to read safetensors file: {}", e)))?;

        let start = info.byte_offset as usize;
        let end = start + info.data_len as usize;

        if end > file_data.len() {
            return Err(AosError::Parse(
                "Tensor data extends beyond file".to_string(),
            ));
        }

        Ok(file_data[start..end].to_vec())
    }

    /// Load a tensor and convert to float32
    pub fn load_tensor_as_f32(&self, name: &str) -> Result<(Vec<f32>, Vec<i32>)> {
        let info = self
            .get_tensor_info(name)
            .ok_or_else(|| AosError::NotFound(format!("Tensor not found: {}", name)))?;

        let data = self.load_tensor_data(name)?;

        let values = match info.dtype.as_str() {
            "float32" | "F32" => {
                // Convert bytes to f32
                if data.len() % 4 != 0 {
                    return Err(AosError::Parse(
                        "Float32 tensor size not divisible by 4".to_string(),
                    ));
                }
                data.chunks(4)
                    .map(|chunk| {
                        let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
                        f32::from_le_bytes(bytes)
                    })
                    .collect()
            }
            "float16" | "F16" => {
                // Convert f16 to f32 (simplified: skip half.rs dependency)
                return Err(AosError::Validation(
                    "Float16 not yet supported - convert model to float32 first".to_string(),
                ));
            }
            "int8" | "I8" => {
                // Convert i8 to f32
                data.iter().map(|&b| (b as i8) as f32 / 127.0).collect()
            }
            "int4" | "I4" => {
                // Unpack INT4 (2 values per byte)
                let mut values = Vec::new();
                for &byte in &data {
                    let val1 = ((byte & 0x0F) as i8) as f32 / 7.0;
                    let val2 = (((byte >> 4) & 0x0F) as i8) as f32 / 7.0;
                    values.push(val1);
                    values.push(val2);
                }
                // Trim to correct size if needed
                values.truncate(info.shape.iter().map(|&s| s as usize).product::<usize>());
                values
            }
            _ => {
                return Err(AosError::Validation(format!(
                    "Unsupported dtype: {}",
                    info.dtype
                )))
            }
        };

        Ok((values, info.shape.clone()))
    }

    /// Load multiple tensors efficiently
    pub fn load_tensors(&self, names: &[&str]) -> Result<HashMap<String, (Vec<f32>, Vec<i32>)>> {
        let mut result = HashMap::new();
        for name in names {
            let (data, shape) = self.load_tensor_as_f32(name)?;
            result.insert(name.to_string(), (data, shape));
        }
        Ok(result)
    }

    /// Get file metadata
    pub fn file_info(&self) -> FileSummary {
        let mut total_size = 0u64;
        let mut dtype_counts: HashMap<String, usize> = HashMap::new();

        for info in self.tensors.values() {
            total_size += info.data_len;
            *dtype_counts.entry(info.dtype.clone()).or_insert(0) += 1;
        }

        FileSummary {
            file_path: self.file_path.clone(),
            num_tensors: self.tensors.len(),
            total_data_size: total_size,
            dtype_distribution: dtype_counts,
        }
    }

    /// Compute the number of bytes needed for a tensor
    fn compute_data_length(shape: &[i32], dtype: &str) -> Result<u64> {
        let element_count: u64 = shape.iter().map(|&s| s as u64).product();

        let bytes_per_element = match dtype {
            "float32" | "F32" | "int32" | "I32" => 4,
            "float16" | "F16" | "int16" | "I16" => 2,
            "int8" | "I8" => 1,
            "int4" | "I4" => {
                // INT4 is packed: 2 values per byte
                return Ok((element_count + 1) / 2);
            }
            _ => return Err(AosError::Validation(format!("Unknown dtype: {}", dtype))),
        };

        Ok(element_count * bytes_per_element)
    }
}

/// Information about tensor descriptor in safetensors header
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TensorDescription {
    /// Data type string
    pub dtype: String,
    /// Tensor shape
    pub shape: Vec<i32>,
}

/// Summary of safetensors file contents
#[derive(Debug, Clone)]
pub struct FileSummary {
    /// File path
    pub file_path: std::path::PathBuf,
    /// Number of tensors
    pub num_tensors: usize,
    /// Total data size in bytes
    pub total_data_size: u64,
    /// Count of tensors by dtype
    pub dtype_distribution: HashMap<String, usize>,
}

impl FileSummary {
    /// Get total file size in MB
    pub fn size_mb(&self) -> f32 {
        self.total_data_size as f32 / (1024.0 * 1024.0)
    }

    /// Get average tensor size in bytes
    pub fn average_tensor_size(&self) -> u64 {
        if self.num_tensors == 0 {
            0
        } else {
            self.total_data_size / self.num_tensors as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_info_creation() {
        let info = TensorInfo {
            name: "test_tensor".to_string(),
            dtype: "float32".to_string(),
            shape: vec![2, 3, 4],
            byte_offset: 100,
            data_len: 96, // 2*3*4*4 bytes
        };

        assert_eq!(info.name, "test_tensor");
        assert_eq!(info.data_len, 96);
    }

    #[test]
    fn test_file_summary_size_mb() {
        let summary = FileSummary {
            file_path: std::path::PathBuf::from("test.safetensors"),
            num_tensors: 10,
            total_data_size: 1024 * 1024,
            dtype_distribution: HashMap::new(),
        };

        assert!((summary.size_mb() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_file_summary_average_size() {
        let summary = FileSummary {
            file_path: std::path::PathBuf::from("test.safetensors"),
            num_tensors: 4,
            total_data_size: 400,
            dtype_distribution: HashMap::new(),
        };

        assert_eq!(summary.average_tensor_size(), 100);
    }

    #[test]
    fn test_compute_data_length_float32() {
        let length = SafetensorsLoader::compute_data_length(&[2, 3, 4], "float32").unwrap();
        assert_eq!(length, 96); // 2*3*4*4
    }

    #[test]
    fn test_compute_data_length_int8() {
        let length = SafetensorsLoader::compute_data_length(&[2, 3, 4], "int8").unwrap();
        assert_eq!(length, 24); // 2*3*4*1
    }

    #[test]
    fn test_compute_data_length_int4() {
        let length = SafetensorsLoader::compute_data_length(&[8], "int4").unwrap();
        assert_eq!(length, 4); // 8 values / 2 per byte = 4 bytes
    }

    #[test]
    fn test_invalid_dtype() {
        let result = SafetensorsLoader::compute_data_length(&[2, 3, 4], "invalid");
        assert!(result.is_err());
    }
}
