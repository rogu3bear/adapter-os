//! AOS v2 Archive Loader for MLX Backend
//!
//! Loads .aos archives (Adapter Object Storage v2) and converts weights to MLX tensors.
//!
//! ## Features
//!
//! - Zero-copy memory-mapped loading via AosV2Parser
//! - Safetensors to MLX tensor conversion
//! - Tensor name mapping (safetensors → MLX module names)
//! - BLAKE3 hash verification
//! - Proper error handling with AosError
//!
//! ## Example
//!
//! ```ignore
//! use adapteros_lora_mlx_ffi::aos_loader::AosLoader;
//!
//! let loader = AosLoader::new();
//! let adapter = loader.load_from_aos("adapter.aos")?;
//! ```

use crate::{LoRAAdapter, LoRAConfig};
use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// AOS v2 loader for MLX backend
///
/// Handles loading .aos archives and converting weights to MLX-compatible format.
pub struct AosLoader {
    /// Enable hash verification (default: true)
    verify_hashes: bool,
    /// Enable strict tensor shape validation (default: true)
    strict_validation: bool,
}

impl Default for AosLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl AosLoader {
    /// Create a new AOS loader with default settings
    pub fn new() -> Self {
        Self {
            verify_hashes: true,
            strict_validation: true,
        }
    }

    /// Create a new AOS loader with custom settings
    pub fn with_options(verify_hashes: bool, strict_validation: bool) -> Self {
        Self {
            verify_hashes,
            strict_validation,
        }
    }

    /// Load a LoRA adapter from an .aos file
    ///
    /// Args:
    /// - `path`: Path to .aos archive
    ///
    /// Errors:
    /// - `AosError::NotFound` if file doesn't exist
    /// - `AosError::Validation` if .aos format is invalid
    /// - `AosError::Verification` if hash verification fails
    /// - `AosError::Parse` if tensor conversion fails
    pub fn load_from_aos<P: AsRef<Path>>(&self, path: P) -> Result<LoRAAdapter> {
        let path = path.as_ref();
        info!(path = %path.display(), "Loading LoRA adapter from .aos archive");

        // Check file exists
        if !path.exists() {
            return Err(AosError::NotFound(format!(
                "AOS file not found: {}",
                path.display()
            )));
        }

        // Parse AOS archive - requires mmap feature from adapteros-aos
        #[cfg(feature = "mmap")]
        {
            self.load_with_parser(path)
        }

        #[cfg(not(feature = "mmap"))]
        {
            Err(AosError::Config(
                "AOS loading requires 'mmap' feature to be enabled in adapteros-aos".to_string(),
            ))
        }
    }

    #[cfg(feature = "mmap")]
    fn load_with_parser<P: AsRef<Path>>(&self, path: P) -> Result<LoRAAdapter> {
        use adapteros_aos::aos_v2_parser::{AosV2Manifest, AosV2Parser};

        let path = path.as_ref();
        let mut parser = AosV2Parser::open(path)?;

        // Parse manifest
        let manifest: AosV2Manifest = parser.manifest()?;
        manifest.validate()?;

        info!(
            adapter_id = %manifest.adapter_id,
            rank = manifest.rank,
            version = %manifest.version,
            "Parsed AOS manifest"
        );

        // Verify hash if enabled and present in manifest
        if self.verify_hashes {
            if let Some(ref weights_hash) = manifest.weights_hash {
                parser.verify_hash(weights_hash)?;
                debug!(hash = %weights_hash.to_short_hex(), "Hash verification passed");
            } else {
                warn!("No weights hash in manifest, skipping verification");
            }
        }

        // Get tensor metadata
        let tensor_metadata = parser.tensor_metadata()?;
        debug!(
            tensor_count = tensor_metadata.len(),
            "Extracted tensor metadata from safetensors"
        );

        // Create LoRA config from manifest
        let config = LoRAConfig {
            rank: manifest.rank as usize,
            alpha: manifest
                .metadata
                .get("alpha")
                .and_then(|v| v.as_f64())
                .unwrap_or(16.0) as f32,
            target_modules: manifest
                .metadata
                .get("target_modules")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![
                        "q_proj".to_string(),
                        "k_proj".to_string(),
                        "v_proj".to_string(),
                        "o_proj".to_string(),
                    ]
                }),
            dropout: manifest
                .metadata
                .get("dropout")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.1) as f32,
        };

        // Create adapter
        let weights_hash = manifest
            .weights_hash
            .clone()
            .unwrap_or_else(|| B3Hash::hash(manifest.adapter_id.as_bytes()));
        let mut adapter = LoRAAdapter::new(manifest.adapter_id.clone(), config.clone());
        adapter.hash = weights_hash;

        // Load weights from safetensors
        self.load_weights_from_safetensors(&mut parser, &mut adapter, &config)?;

        info!(
            adapter_id = %adapter.id,
            parameter_count = adapter.parameter_count(),
            memory_usage_mb = adapter.memory_usage() as f32 / (1024.0 * 1024.0),
            "Successfully loaded LoRA adapter from .aos"
        );

        Ok(adapter)
    }

    #[cfg(feature = "mmap")]
    fn load_weights_from_safetensors(
        &self,
        parser: &mut adapteros_aos::aos_v2_parser::AosV2Parser,
        adapter: &mut LoRAAdapter,
        config: &LoRAConfig,
    ) -> Result<()> {
        use adapteros_aos::aos_v2_parser::TensorView;

        // Get all tensor names from the archive
        let tensor_names = parser.tensor_names()?;
        debug!(
            tensor_count = tensor_names.len(),
            "Found tensors in safetensors archive"
        );

        // First pass: identify tensor names
        let mut shared_down_name: Option<String> = None;
        let mut tensor_map: HashMap<String, HashMap<String, String>> = HashMap::new();

        for name in &tensor_names {
            // Check for shared down projection (new architecture)
            if name.contains("shared_down") {
                if shared_down_name.is_none() {
                    shared_down_name = Some(name.clone());
                    debug!(tensor_name = %name, "Found shared down projection");
                    continue;
                }
            }

            // Parse tensor name: format is typically "model.layers.0.self_attn.q_proj.lora_A"
            if let Some((module_name, matrix_type)) = self.parse_tensor_name(name) {
                tensor_map
                    .entry(module_name)
                    .or_insert_with(HashMap::new)
                    .insert(matrix_type, name.clone());
            } else {
                debug!(tensor_name = %name, "Skipping non-LoRA tensor");
            }
        }

        // Load shared down projection if present
        if let Some(ref name) = shared_down_name {
            if let Some(shared_down_view) = parser.tensor(name)? {
                let shared_down_matrix = self.tensor_to_matrix(&shared_down_view)?;
                adapter
                    .set_shared_down(shared_down_matrix)
                    .map_err(|e| AosError::Validation(format!("Failed to set shared_down: {}", e)))?;
                debug!(
                    shared_down_shape = ?(shared_down_view.shape()),
                    "Loaded shared down projection"
                );
            }
        }

        // Second pass: load tensors and add to adapter
        for module_name in &config.target_modules {
            if let Some(module_tensors) = tensor_map.get(module_name) {
                // Get lora_A and lora_B tensor names
                let lora_a_name = module_tensors.get("lora_A");
                let lora_b_name = module_tensors.get("lora_B");

                match (lora_a_name, lora_b_name) {
                    (Some(a_name), Some(b_name)) => {
                        // Load and convert A tensor
                        let a_matrix = {
                            let a_tensor = parser.tensor(a_name)?.ok_or_else(|| {
                                AosError::Validation(format!("Tensor {} not found", a_name))
                            })?;
                            self.tensor_to_matrix(&a_tensor)?
                        };

                        // Load and convert B tensor
                        let b_matrix = {
                            let b_tensor = parser.tensor(b_name)?.ok_or_else(|| {
                                AosError::Validation(format!("Tensor {} not found", b_name))
                            })?;
                            self.tensor_to_matrix(&b_tensor)?
                        };

                        // Validate shapes if strict validation enabled
                        if self.strict_validation {
                            self.validate_lora_shapes(&a_matrix, &b_matrix, config.rank)?;
                        }

                        // Use legacy add method (converts A to shared if first module)
                        adapter.add_module_weights_legacy(module_name, a_matrix.clone(), b_matrix.clone());
                        debug!(
                            module = %module_name,
                            "Loaded LoRA weights for module (legacy format)"
                        );
                    }
                    (None, Some(b_name)) => {
                        // New architecture: only B matrices, shared_down already loaded
                        let b_matrix = {
                            let b_tensor = parser.tensor(b_name)?.ok_or_else(|| {
                                AosError::Validation(format!("Tensor {} not found", b_name))
                            })?;
                            self.tensor_to_matrix(&b_tensor)?
                        };
                        adapter.add_module_weights(module_name, b_matrix);
                        debug!(
                            module = %module_name,
                            "Loaded LoRA up-projection for module (shared down architecture)"
                        );
                    }
                    _ => {
                        debug!(
                            module = %module_name,
                            "Missing lora_B tensor, skipping module"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse tensor name to extract module name and matrix type
    ///
    /// Examples:
    /// - "model.layers.0.self_attn.q_proj.lora_A" -> ("q_proj", "lora_A")
    /// - "q_proj.lora_B" -> ("q_proj", "lora_B")
    /// - "lora_A" -> ("default", "lora_A")
    fn parse_tensor_name(&self, name: &str) -> Option<(String, String)> {
        // Split by '.'
        let parts: Vec<&str> = name.split('.').collect();

        // Look for lora_A or lora_B
        if let Some(pos) = parts.iter().position(|&p| p == "lora_A" || p == "lora_B") {
            let matrix_type = parts[pos].to_string();

            // Extract module name (the part before lora_A/lora_B)
            let module_name = if pos > 0 {
                parts[pos - 1].to_string()
            } else {
                "default".to_string()
            };

            Some((module_name, matrix_type))
        } else {
            None
        }
    }

    /// Convert TensorView to f32 matrix
    ///
    /// Handles all supported data types using the enhanced dtype_convert module
    #[cfg(feature = "mmap")]
    fn tensor_to_matrix(
        &self,
        tensor: &adapteros_aos::aos_v2_parser::TensorView,
    ) -> Result<Vec<Vec<f32>>> {
        use crate::dtype_convert::*;
        use crate::safetensors_loader::DType;

        // Validate shape (should be 2D for LoRA matrices)
        if tensor.shape.len() != 2 {
            return Err(AosError::Parse(format!(
                "Invalid tensor shape: expected 2D, got {:?}",
                tensor.shape
            )));
        }

        let rows = tensor.shape[0];
        let cols = tensor.shape[1];
        let n_elements = rows * cols;
        let data = tensor.as_bytes();

        // Parse dtype
        let dtype = DType::from_str(&tensor.dtype)?;

        // Convert to f32 using dtype_convert module
        let floats = match dtype {
            DType::F32 => {
                // Direct conversion from bytes to f32
                data.chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            }
            DType::F16 => f16_to_f32(data)?,
            DType::BF16 => bf16_to_f32(data)?,
            DType::I8 => i8_to_f32(data)?,
            DType::U8 => u8_to_f32(data)?,
            DType::I16 => i16_to_f32(data)?,
            DType::I32 => i32_to_f32(data)?,
            DType::Q4_0 => dequantize_q4_0(data, n_elements)?,
            DType::Q4_1 => dequantize_q4_1(data, n_elements)?,
            DType::Q8_0 => dequantize_q8_0(data, n_elements)?,
        };

        // Reshape to 2D matrix
        let mut matrix = Vec::with_capacity(rows);
        for row_idx in 0..rows {
            let start = row_idx * cols;
            let end = start + cols;
            matrix.push(floats[start..end].to_vec());
        }

        tracing::debug!(
            shape = ?(rows, cols),
            dtype = ?dtype,
            "Converted tensor to f32 matrix"
        );

        Ok(matrix)
    }

    /// Validate LoRA matrix shapes
    ///
    /// Args:
    /// - `lora_a`: Down-projection matrix (hidden_dim x rank)
    /// - `lora_b`: Up-projection matrix (rank x hidden_dim)
    /// - `expected_rank`: Expected LoRA rank
    ///
    /// Errors:
    /// - `AosError::Validation` if shapes are incompatible
    fn validate_lora_shapes(
        &self,
        lora_a: &[Vec<f32>],
        lora_b: &[Vec<f32>],
        expected_rank: usize,
    ) -> Result<()> {
        if lora_a.is_empty() || lora_b.is_empty() {
            return Err(AosError::Validation(
                "LoRA matrices cannot be empty".to_string(),
            ));
        }

        let a_rows = lora_a.len();
        let a_cols = lora_a[0].len();
        let b_rows = lora_b.len();
        let b_cols = lora_b[0].len();

        // lora_A should be (hidden_dim x rank)
        // lora_B should be (rank x hidden_dim)
        if a_cols != expected_rank {
            return Err(AosError::Validation(format!(
                "lora_A shape mismatch: expected rank={}, got cols={}",
                expected_rank, a_cols
            )));
        }

        if b_rows != expected_rank {
            return Err(AosError::Validation(format!(
                "lora_B shape mismatch: expected rank={}, got rows={}",
                expected_rank, b_rows
            )));
        }

        // hidden_dim should match between A and B
        if a_rows != b_cols {
            return Err(AosError::Validation(format!(
                "LoRA shape mismatch: lora_A rows ({}) != lora_B cols ({})",
                a_rows, b_cols
            )));
        }

        debug!(
            lora_a_shape = ?(a_rows, a_cols),
            lora_b_shape = ?(b_rows, b_cols),
            rank = expected_rank,
            "LoRA shape validation passed"
        );

        Ok(())
    }

    /// Load multiple adapters from .aos files into a HashMap
    ///
    /// Args:
    /// - `adapter_paths`: List of (adapter_id, path) tuples
    ///
    /// Returns:
    /// - HashMap of adapter_id -> Arc<LoRAAdapter>
    ///
    /// Errors:
    /// - Returns first encountered error if any adapter fails to load
    pub fn load_multiple<P: AsRef<Path>>(
        &self,
        adapter_paths: &[(u16, P)],
    ) -> Result<HashMap<u16, Arc<LoRAAdapter>>> {
        let mut adapters = HashMap::new();

        for (adapter_id, path) in adapter_paths {
            let adapter = self.load_from_aos(path)?;
            adapters.insert(*adapter_id, Arc::new(adapter));
        }

        info!(
            adapter_count = adapters.len(),
            "Loaded multiple LoRA adapters from .aos archives"
        );

        Ok(adapters)
    }

    /// Load adapter and verify it matches expected hash
    ///
    /// Args:
    /// - `path`: Path to .aos file
    /// - `expected_hash`: Expected BLAKE3 hash
    ///
    /// Errors:
    /// - `AosError::Verification` if hash doesn't match
    pub fn load_and_verify<P: AsRef<Path>>(
        &self,
        path: P,
        expected_hash: &B3Hash,
    ) -> Result<LoRAAdapter> {
        let adapter = self.load_from_aos(path)?;

        if &adapter.hash != expected_hash {
            return Err(AosError::Verification(format!(
                "Adapter hash mismatch: expected {}, got {}",
                expected_hash.to_hex(),
                adapter.hash.to_hex()
            )));
        }

        Ok(adapter)
    }
}

/// Integration with MLXFFIBackend for loading adapters
pub trait MlxBackendAosExt {
    /// Load adapter from .aos file and register with backend
    ///
    /// Args:
    /// - `adapter_id`: Unique adapter ID for registration
    /// - `aos_path`: Path to .aos archive
    ///
    /// Errors:
    /// - `AosError::*` if loading fails
    fn load_adapter_from_aos<P: AsRef<Path>>(&self, adapter_id: u16, aos_path: P) -> Result<()>;

    /// Load multiple adapters from .aos files
    ///
    /// Args:
    /// - `adapter_paths`: List of (adapter_id, path) tuples
    fn load_adapters_from_aos<P: AsRef<Path>>(&self, adapter_paths: &[(u16, P)]) -> Result<()>;
}

impl MlxBackendAosExt for crate::backend::MLXFFIBackend {
    fn load_adapter_from_aos<P: AsRef<Path>>(&self, adapter_id: u16, aos_path: P) -> Result<()> {
        let loader = AosLoader::new();
        let adapter = loader.load_from_aos(aos_path)?;
        self.register_adapter(adapter_id, adapter)
    }

    fn load_adapters_from_aos<P: AsRef<Path>>(&self, adapter_paths: &[(u16, P)]) -> Result<()> {
        for (adapter_id, path) in adapter_paths {
            self.load_adapter_from_aos(*adapter_id, path)?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "mmap"))]
mod tests {
    use super::*;
    use adapteros_aos::aos2_writer::AOS2Writer;
    use adapteros_aos::aos_v2_parser::AosV2Manifest;
    use tempfile::NamedTempFile;

    fn create_test_aos_file() -> Result<NamedTempFile> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        // Create manifest
        let manifest = AosV2Manifest {
            version: "2.0".to_string(),
            adapter_id: "test-adapter".to_string(),
            rank: 8,
            weights_hash: None,
            tensor_shapes: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("alpha".to_string(), serde_json::json!(16.0));
                m.insert(
                    "target_modules".to_string(),
                    serde_json::json!(["q_proj", "v_proj"]),
                );
                m
            },
        };

        // Create minimal safetensors data
        // Format: [header_size:u64][header_json][tensor_data]
        let header_json = serde_json::json!({
            "q_proj.lora_A": {
                "dtype": "F32",
                "shape": [768, 8],
                "data_offsets": [0, 24576]
            },
            "q_proj.lora_B": {
                "dtype": "F32",
                "shape": [8, 768],
                "data_offsets": [24576, 49152]
            }
        });

        let header_bytes = serde_json::to_vec(&header_json).unwrap();
        let header_size = header_bytes.len() as u64;

        let mut weights_data = Vec::new();
        weights_data.extend_from_slice(&header_size.to_le_bytes());
        weights_data.extend_from_slice(&header_bytes);
        // Add fake tensor data (zeros)
        weights_data.extend_from_slice(&vec![0u8; 49152]);

        // Write archive
        let writer = AOS2Writer::new();
        writer.write_archive(temp_file.path(), &manifest, &weights_data)?;

        Ok(temp_file)
    }

    #[test]
    fn test_aos_loader_basic() -> Result<()> {
        let temp_file = create_test_aos_file()?;
        let loader = AosLoader::new();
        let adapter = loader.load_from_aos(temp_file.path())?;

        assert_eq!(adapter.id(), "test-adapter");
        assert_eq!(adapter.config().rank, 8);

        Ok(())
    }

    #[test]
    fn test_aos_loader_missing_file() {
        let loader = AosLoader::new();
        let result = loader.load_from_aos("/nonexistent/file.aos");

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AosError::NotFound(_)));
    }

    #[test]
    fn test_tensor_name_parsing() {
        let loader = AosLoader::new();

        // Standard format
        assert_eq!(
            loader.parse_tensor_name("model.layers.0.self_attn.q_proj.lora_A"),
            Some(("q_proj".to_string(), "lora_A".to_string()))
        );

        // Simple format
        assert_eq!(
            loader.parse_tensor_name("q_proj.lora_B"),
            Some(("q_proj".to_string(), "lora_B".to_string()))
        );

        // Minimal format
        assert_eq!(
            loader.parse_tensor_name("lora_A"),
            Some(("default".to_string(), "lora_A".to_string()))
        );

        // Non-LoRA tensor
        assert_eq!(loader.parse_tensor_name("model.embed_tokens.weight"), None);
    }

    #[test]
    fn test_shape_validation() {
        let loader = AosLoader::new();

        // Valid shapes
        let lora_a = vec![vec![1.0; 8]; 768]; // 768 x 8
        let lora_b = vec![vec![1.0; 768]; 8]; // 8 x 768
        assert!(loader.validate_lora_shapes(&lora_a, &lora_b, 8).is_ok());

        // Invalid rank
        let lora_a_bad = vec![vec![1.0; 16]; 768]; // Wrong rank
        assert!(loader
            .validate_lora_shapes(&lora_a_bad, &lora_b, 8)
            .is_err());

        // Incompatible dimensions
        let lora_b_bad = vec![vec![1.0; 512]; 8]; // Wrong hidden_dim
        assert!(loader
            .validate_lora_shapes(&lora_a, &lora_b_bad, 8)
            .is_err());
    }

    #[test]
    fn test_load_multiple_adapters() -> Result<()> {
        let temp_file1 = create_test_aos_file()?;
        let temp_file2 = create_test_aos_file()?;

        let loader = AosLoader::new();
        let adapter_paths = vec![(1u16, temp_file1.path()), (2u16, temp_file2.path())];

        let adapters = loader.load_multiple(&adapter_paths)?;

        assert_eq!(adapters.len(), 2);
        assert!(adapters.contains_key(&1));
        assert!(adapters.contains_key(&2));

        Ok(())
    }
}
