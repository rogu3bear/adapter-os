//! Adapter Loading System for CoreML Backend
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This module implements .aos file loading and tensor conversion for the CoreML backend.
//! It handles parsing safetensors, converting to MLMultiArray format, and managing
//! adapter lifecycle for Neural Engine acceleration.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────┐
//! │             AdapterLoader (Rust)                            │
//! │  - Parse .aos files using AosV2Parser                       │
//! │  - Extract LoRA A/B matrices from safetensors               │
//! │  - Convert to CoreML MLMultiArray format                    │
//! │  - Handle dtype conversions (F32, F16, INT8)                │
//! │  - Memory-efficient streaming                               │
//! └──────────────────────┬─────────────────────────────────────┘
//!                        │
//!                        │ FFI calls
//!                        ▼
//! ┌────────────────────────────────────────────────────────────┐
//! │         CoreML FFI Layer (Objective-C++)                    │
//! │  - Create MLMultiArray from raw pointer                     │
//! │  - Upload to ANE memory                                     │
//! │  - Manage adapter buffers                                   │
//! └────────────────────────────────────────────────────────────┘
//! ```

use adapteros_aos::aos_v2_parser::{AosV2Parser, AosV2Manifest};
use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

/// Tensor data type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F16,
    INT8,
}

impl DType {
    /// Parse dtype from safetensors string
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "F32" | "Float32" => Ok(DType::F32),
            "F16" | "Float16" => Ok(DType::F16),
            "I8" | "INT8" | "Int8" => Ok(DType::INT8),
            _ => Err(AosError::Validation(format!("Unknown dtype: {}", s))),
        }
    }

    /// Get element size in bytes
    pub fn element_size(&self) -> usize {
        match self {
            DType::F32 => 4,
            DType::F16 => 2,
            DType::INT8 => 1,
        }
    }
}

/// Converted tensor ready for CoreML
#[derive(Debug)]
pub struct CoreMLTensor {
    /// Tensor name (e.g., "q_proj.lora_A")
    pub name: String,
    /// Tensor shape (e.g., [rank, in_dim])
    pub shape: Vec<usize>,
    /// Data type
    pub dtype: DType,
    /// Raw tensor data (dtype-dependent encoding)
    pub data: Vec<u8>,
}

impl CoreMLTensor {
    /// Get number of elements
    pub fn num_elements(&self) -> usize {
        self.shape.iter().product()
    }

    /// Get data as f32 slice (only valid if dtype == F32)
    pub fn as_f32(&self) -> Option<&[f32]> {
        if self.dtype != DType::F32 {
            return None;
        }
        Some(unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const f32,
                self.data.len() / 4,
            )
        })
    }

    /// Get data as f16 slice (only valid if dtype == F16)
    pub fn as_f16(&self) -> Option<&[u16]> {
        if self.dtype != DType::F16 {
            return None;
        }
        Some(unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const u16,
                self.data.len() / 2,
            )
        })
    }

    /// Get data as i8 slice (only valid if dtype == INT8)
    pub fn as_i8(&self) -> Option<&[i8]> {
        if self.dtype != DType::INT8 {
            return None;
        }
        Some(unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const i8,
                self.data.len(),
            )
        })
    }
}

/// LoRA adapter weights for CoreML
#[derive(Debug)]
pub struct CoreMLAdapter {
    /// Adapter ID
    pub adapter_id: String,
    /// LoRA rank
    pub rank: usize,
    /// LoRA alpha
    pub alpha: f32,
    /// LoRA A matrices (one per target module)
    /// Order: [q_proj, k_proj, v_proj, mlp.down_proj, mlp.up_proj]
    pub lora_a_tensors: Vec<CoreMLTensor>,
    /// LoRA B matrices (one per target module)
    /// Order: [q_proj, k_proj, v_proj, mlp.down_proj, mlp.up_proj]
    pub lora_b_tensors: Vec<CoreMLTensor>,
    /// Content hash for verification
    pub hash_b3: B3Hash,
    /// Total bytes (estimated)
    pub total_bytes: usize,
}

impl CoreMLAdapter {
    /// Calculate scaling factor: alpha / rank
    pub fn scaling_factor(&self) -> f32 {
        self.alpha / (self.rank as f32)
    }

    /// Get target module names
    pub fn target_modules() -> &'static [&'static str] {
        &["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"]
    }
}

/// Adapter loader for CoreML backend
pub struct AdapterLoader {
    /// Loaded adapters indexed by ID
    adapters: HashMap<u16, CoreMLAdapter>,
    /// Shared down-projection weights (MPLoRA support)
    shared_down_proj: Option<CoreMLTensor>,
    /// Total memory usage in bytes
    total_memory_bytes: usize,
}

impl AdapterLoader {
    /// Create a new adapter loader
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            shared_down_proj: None,
            total_memory_bytes: 0,
        }
    }

    /// Load adapter from .aos file
    ///
    /// # Arguments
    /// * `id` - Adapter ID (u16)
    /// * `aos_path` - Path to .aos file
    ///
    /// # Process
    /// 1. Parse .aos file using AosV2Parser
    /// 2. Extract manifest and validate
    /// 3. Extract LoRA A/B tensors from safetensors
    /// 4. Convert tensors to CoreML format
    /// 5. Store in adapter registry
    ///
    /// # Errors
    /// - `AosError::Io` if file cannot be read
    /// - `AosError::Validation` if manifest is invalid
    /// - `AosError::Parse` if safetensors format is invalid
    pub fn load_from_file<P: AsRef<Path>>(&mut self, id: u16, aos_path: P) -> Result<()> {
        let path = aos_path.as_ref();
        info!(
            adapter_id = id,
            path = %path.display(),
            "Loading adapter from .aos file"
        );

        // 1. Parse .aos file
        let mut parser = AosV2Parser::open(path)?;

        // 2. Extract and validate manifest
        let manifest: AosV2Manifest = parser.manifest()?;
        manifest.validate()?;

        info!(
            adapter_id = id,
            rank = manifest.rank,
            aos_adapter_id = %manifest.adapter_id,
            "Parsed AOS manifest"
        );

        // 3. Verify hash if present
        if let Some(ref expected_hash) = manifest.weights_hash {
            parser.verify_hash(expected_hash)?;
        }

        // 4. Extract tensor metadata
        let tensor_info = parser.tensor_metadata()?;
        debug!(
            adapter_id = id,
            tensor_count = tensor_info.len(),
            "Extracted tensor metadata"
        );

        // 5. Load LoRA A/B matrices
        let (lora_a_tensors, lora_b_tensors) = self.load_lora_tensors(&mut parser, manifest.rank)?;

        // 6. Calculate alpha (from manifest or default)
        let alpha = manifest
            .metadata
            .get("lora_alpha")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or_else(|| {
                let default_alpha = (2 * manifest.rank) as f32;
                warn!(
                    adapter_id = id,
                    rank = manifest.rank,
                    default_alpha = default_alpha,
                    "No lora_alpha in manifest, using 2*rank"
                );
                default_alpha
            });

        // 7. Calculate total bytes
        let total_bytes: usize = lora_a_tensors
            .iter()
            .map(|t| t.data.len())
            .chain(lora_b_tensors.iter().map(|t| t.data.len()))
            .sum();

        // 8. Compute content hash
        let weights_bytes = parser.weights_bytes();
        let hash_b3 = B3Hash::hash(weights_bytes);

        // 9. Create adapter
        let adapter = CoreMLAdapter {
            adapter_id: manifest.adapter_id.clone(),
            rank: manifest.rank as usize,
            alpha,
            lora_a_tensors,
            lora_b_tensors,
            hash_b3: hash_b3.clone(),
            total_bytes,
        };

        // 10. Store adapter
        if let Some(old_adapter) = self.adapters.insert(id, adapter) {
            warn!(
                adapter_id = id,
                "Replaced existing adapter (freed {} bytes)",
                old_adapter.total_bytes
            );
            self.total_memory_bytes -= old_adapter.total_bytes;
        }

        self.total_memory_bytes += total_bytes;

        info!(
            adapter_id = id,
            rank = manifest.rank,
            alpha = alpha,
            total_bytes = total_bytes,
            hash_b3 = %hash_b3.to_short_hex(),
            num_loaded_adapters = self.adapters.len(),
            total_memory_mb = self.total_memory_bytes / (1024 * 1024),
            "Adapter loaded successfully"
        );

        Ok(())
    }

    /// Load adapter from raw bytes (AOS format)
    ///
    /// # Arguments
    /// * `id` - Adapter ID (u16)
    /// * `aos_bytes` - Raw .aos file bytes
    pub fn load_from_bytes(&mut self, id: u16, aos_bytes: &[u8]) -> Result<()> {
        // Write to temporary file and load
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        temp_file
            .write_all(aos_bytes)
            .map_err(|e| AosError::Io(format!("Failed to write temp file: {}", e)))?;

        temp_file
            .flush()
            .map_err(|e| AosError::Io(format!("Failed to flush temp file: {}", e)))?;

        self.load_from_file(id, temp_file.path())
    }

    /// Load LoRA A/B tensors from parsed AOS file
    fn load_lora_tensors(
        &self,
        parser: &mut AosV2Parser,
        rank: u32,
    ) -> Result<(Vec<CoreMLTensor>, Vec<CoreMLTensor>)> {
        let target_modules = CoreMLAdapter::target_modules();
        let mut lora_a_tensors = Vec::with_capacity(target_modules.len());
        let mut lora_b_tensors = Vec::with_capacity(target_modules.len());

        for &module in target_modules {
            let a_name = format!("{}.lora_A", module);
            let b_name = format!("{}.lora_B", module);

            // Load A matrix
            if let Some(a_tensor_view) = parser.tensor(&a_name)? {
                let a_tensor = self.convert_tensor(&a_name, &a_tensor_view)?;
                lora_a_tensors.push(a_tensor);
            } else {
                // Create zero buffer as fallback
                warn!(module = %module, "LoRA A matrix not found, creating zero buffer");
                let zero_tensor = self.create_zero_tensor(&a_name, rank as usize, 4096)?;
                lora_a_tensors.push(zero_tensor);
            }

            // Load B matrix
            if let Some(b_tensor_view) = parser.tensor(&b_name)? {
                let b_tensor = self.convert_tensor(&b_name, &b_tensor_view)?;
                lora_b_tensors.push(b_tensor);
            } else {
                // Create zero buffer as fallback
                warn!(module = %module, "LoRA B matrix not found, creating zero buffer");
                let zero_tensor = self.create_zero_tensor(&b_name, 4096, rank as usize)?;
                lora_b_tensors.push(zero_tensor);
            }
        }

        Ok((lora_a_tensors, lora_b_tensors))
    }

    /// Convert safetensors tensor to CoreML format
    fn convert_tensor(
        &self,
        name: &str,
        tensor_view: &adapteros_aos::aos_v2_parser::TensorView,
    ) -> Result<CoreMLTensor> {
        // Parse dtype from tensor metadata
        let dtype = DType::from_str(&tensor_view.dtype)?;

        // Get raw bytes
        let raw_data = tensor_view.as_bytes();

        // Convert to CoreML format based on dtype
        let converted_data = match dtype {
            DType::F32 => {
                // F32 -> F32 (no conversion needed, just copy)
                raw_data.to_vec()
            }
            DType::F16 => {
                // F16 -> F16 (no conversion needed for CoreML)
                raw_data.to_vec()
            }
            DType::INT8 => {
                // INT8 -> INT8 (no conversion needed)
                raw_data.to_vec()
            }
        };

        Ok(CoreMLTensor {
            name: name.to_string(),
            shape: tensor_view.shape.clone(),
            dtype,
            data: converted_data,
        })
    }

    /// Create a zero tensor (fallback when tensor is missing)
    fn create_zero_tensor(&self, name: &str, dim0: usize, dim1: usize) -> Result<CoreMLTensor> {
        let shape = vec![dim0, dim1];
        let num_elements = dim0 * dim1;
        let dtype = DType::F32; // Default to F32 for zero tensors

        let data = vec![0u8; num_elements * dtype.element_size()];

        Ok(CoreMLTensor {
            name: name.to_string(),
            shape,
            dtype,
            data,
        })
    }

    /// Unload adapter
    ///
    /// # Arguments
    /// * `id` - Adapter ID to unload
    ///
    /// # Returns
    /// Ok(()) on success
    ///
    /// # Errors
    /// - `AosError::NotFound` if adapter is not loaded
    pub fn unload(&mut self, id: u16) -> Result<()> {
        if let Some(adapter) = self.adapters.remove(&id) {
            self.total_memory_bytes -= adapter.total_bytes;
            info!(
                adapter_id = id,
                freed_bytes = adapter.total_bytes,
                freed_mb = adapter.total_bytes / (1024 * 1024),
                num_remaining_adapters = self.adapters.len(),
                total_memory_mb = self.total_memory_bytes / (1024 * 1024),
                "Adapter unloaded"
            );
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Adapter {} not loaded", id)))
        }
    }

    /// Get adapter by ID
    pub fn get(&self, id: u16) -> Option<&CoreMLAdapter> {
        self.adapters.get(&id)
    }

    /// Get mutable adapter by ID
    pub fn get_mut(&mut self, id: u16) -> Option<&mut CoreMLAdapter> {
        self.adapters.get_mut(&id)
    }

    /// Check if adapter is loaded
    pub fn is_loaded(&self, id: u16) -> bool {
        self.adapters.contains_key(&id)
    }

    /// Get number of loaded adapters
    pub fn num_loaded(&self) -> usize {
        self.adapters.len()
    }

    /// Get total memory usage in bytes
    pub fn total_memory_bytes(&self) -> usize {
        self.total_memory_bytes
    }

    /// Get all loaded adapter IDs
    pub fn loaded_ids(&self) -> Vec<u16> {
        self.adapters.keys().copied().collect()
    }

    /// Clear all loaded adapters
    pub fn clear(&mut self) {
        let num_adapters = self.adapters.len();
        let freed_bytes = self.total_memory_bytes;
        self.adapters.clear();
        self.total_memory_bytes = 0;
        info!(
            num_adapters = num_adapters,
            freed_mb = freed_bytes / (1024 * 1024),
            "Cleared all adapters"
        );
    }

    /// Set shared down-projection matrix (MPLoRA support)
    pub fn set_shared_down_proj(&mut self, tensor: CoreMLTensor) {
        info!(
            shape = ?tensor.shape,
            dtype = ?tensor.dtype,
            bytes = tensor.data.len(),
            "Set shared down-projection matrix"
        );
        self.shared_down_proj = Some(tensor);
    }

    /// Get shared down-projection matrix
    pub fn shared_down_proj(&self) -> Option<&CoreMLTensor> {
        self.shared_down_proj.as_ref()
    }
}

impl Default for AdapterLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_aos::aos2_writer::AOS2Writer;
    use tempfile::NamedTempFile;

    #[test]
    fn test_dtype_parsing() {
        assert_eq!(DType::from_str("F32").unwrap(), DType::F32);
        assert_eq!(DType::from_str("Float32").unwrap(), DType::F32);
        assert_eq!(DType::from_str("F16").unwrap(), DType::F16);
        assert_eq!(DType::from_str("I8").unwrap(), DType::INT8);
        assert!(DType::from_str("Unknown").is_err());
    }

    #[test]
    fn test_dtype_element_size() {
        assert_eq!(DType::F32.element_size(), 4);
        assert_eq!(DType::F16.element_size(), 2);
        assert_eq!(DType::INT8.element_size(), 1);
    }

    #[test]
    fn test_adapter_loader_creation() {
        let loader = AdapterLoader::new();
        assert_eq!(loader.num_loaded(), 0);
        assert_eq!(loader.total_memory_bytes(), 0);
    }

    #[test]
    fn test_create_zero_tensor() {
        let loader = AdapterLoader::new();
        let tensor = loader
            .create_zero_tensor("test.lora_A", 8, 768)
            .unwrap();

        assert_eq!(tensor.name, "test.lora_A");
        assert_eq!(tensor.shape, vec![8, 768]);
        assert_eq!(tensor.dtype, DType::F32);
        assert_eq!(tensor.num_elements(), 8 * 768);
        assert_eq!(tensor.data.len(), 8 * 768 * 4); // F32 = 4 bytes

        // Verify all zeros
        let f32_data = tensor.as_f32().unwrap();
        assert!(f32_data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_load_from_aos_file() -> Result<()> {
        // Create a test .aos file
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test-adapter".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: std::collections::HashMap::new(),
        };

        // Create minimal safetensors data
        let header_json = serde_json::json!({
            "q_proj.lora_A": {
                "dtype": "F32",
                "shape": [8, 768],
                "data_offsets": [0, 8 * 768 * 4]
            }
        });
        let header_bytes = serde_json::to_vec(&header_json).unwrap();
        let header_size = header_bytes.len() as u64;

        let mut weights_data = Vec::new();
        weights_data.extend_from_slice(&header_size.to_le_bytes());
        weights_data.extend_from_slice(&header_bytes);
        weights_data.extend_from_slice(&vec![0u8; 8 * 768 * 4]); // Fake tensor data

        // Write .aos file
        let writer = AOS2Writer::new();
        writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        // Load adapter
        let mut loader = AdapterLoader::new();
        loader.load_from_file(0, temp_file.path())?;

        assert_eq!(loader.num_loaded(), 1);
        assert!(loader.is_loaded(0));

        let adapter = loader.get(0).unwrap();
        assert_eq!(adapter.adapter_id, "test-adapter");
        assert_eq!(adapter.rank, 8);

        Ok(())
    }

    #[test]
    fn test_unload_adapter() -> Result<()> {
        let mut loader = AdapterLoader::new();

        // Create a simple zero adapter
        let zero_tensor = loader.create_zero_tensor("test.lora_A", 8, 768)?;
        let total_bytes = zero_tensor.data.len();

        let adapter = CoreMLAdapter {
            adapter_id: "test".to_string(),
            rank: 8,
            alpha: 16.0,
            lora_a_tensors: vec![zero_tensor],
            lora_b_tensors: vec![],
            hash_b3: B3Hash::hash(b"test"),
            total_bytes,
        };

        loader.adapters.insert(0, adapter);
        loader.total_memory_bytes += total_bytes;

        assert_eq!(loader.num_loaded(), 1);

        // Unload
        loader.unload(0)?;
        assert_eq!(loader.num_loaded(), 0);
        assert_eq!(loader.total_memory_bytes(), 0);

        // Unloading again should error
        assert!(loader.unload(0).is_err());

        Ok(())
    }

    #[test]
    fn test_scaling_factor() {
        let adapter = CoreMLAdapter {
            adapter_id: "test".to_string(),
            rank: 8,
            alpha: 16.0,
            lora_a_tensors: vec![],
            lora_b_tensors: vec![],
            hash_b3: B3Hash::hash(b"test"),
            total_bytes: 0,
        };

        assert_eq!(adapter.scaling_factor(), 2.0);
    }
}
