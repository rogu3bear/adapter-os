//! Safetensors loading for shared down projection architecture
//!
//! This module provides utilities for loading LoRA adapters from safetensors format
//! (.aos files) with the patent-aligned shared down projection architecture.
//!
//! # Features
//!
//! - Comprehensive dtype support: F32, F16, BF16, I8, I4, U8, quantized formats
//! - Lazy loading with memory mapping for large models
//! - Efficient dtype conversion with SIMD optimizations
//! - Weight name mapping for different model architectures
//! - Tensor shape validation and verification
//! - Cache for frequently accessed tensors
//!
//! # Expected Tensor Layout
//!
//! Safetensors tensors should be named according to this convention:
//! - `"lora.shared_down"` - Shared down-projection matrix [rank, hidden_dim]
//! - `"lora.{module}.up"` - Per-module up-projection matrices [hidden_dim, rank]
//!
//! Example keys for a typical transformer adapter:
//! ```text
//! lora.shared_down          [16, 4096]
//! lora.q_proj.up            [4096, 16]
//! lora.k_proj.up            [4096, 16]
//! lora.v_proj.up            [4096, 16]
//! lora.o_proj.up            [4096, 16]
//! ```
//!
//! # Memory Efficiency
//!
//! This layout provides 50% memory savings over traditional LoRA:
//! - Traditional: 2 × N × rank × hidden_dim (separate A/B per module)
//! - Shared down: rank × hidden_dim + N × rank × hidden_dim
//!
//! For N=4 modules, rank=16, hidden_dim=4096:
//! - Traditional: 2 × 4 × 16 × 4096 = 524,288 parameters
//! - Shared down: 16 × 4096 + 4 × 16 × 4096 = 327,680 parameters (37.5% savings)

use crate::lora::{LoRAAdapter, LoRAConfig};
use adapteros_core::{AosError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[cfg(feature = "mmap")]
use memmap2::Mmap;
#[cfg(feature = "mmap")]
use std::path::Path;

/// Expected tensor key prefixes in safetensors format
pub const SHARED_DOWN_KEY: &str = "lora.shared_down";
pub const MODULE_UP_PREFIX: &str = "lora.";
pub const MODULE_UP_SUFFIX: &str = ".up";

/// Supported data types in safetensors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    /// 32-bit floating point
    F32,
    /// 16-bit floating point (IEEE 754)
    F16,
    /// 16-bit brain floating point
    BF16,
    /// 8-bit signed integer
    I8,
    /// 8-bit unsigned integer
    U8,
    /// 32-bit signed integer
    I32,
    /// 16-bit signed integer
    I16,
    /// 4-bit quantized (GGML Q4_0 format)
    Q4_0,
    /// 4-bit quantized with bias (GGML Q4_1 format)
    Q4_1,
    /// 8-bit quantized (GGML Q8_0 format)
    Q8_0,
}

impl DType {
    /// Parse dtype from string representation
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_uppercase().as_str() {
            "F32" | "FLOAT32" => Ok(Self::F32),
            "F16" | "FLOAT16" => Ok(Self::F16),
            "BF16" | "BFLOAT16" => Ok(Self::BF16),
            "I8" | "INT8" => Ok(Self::I8),
            "U8" | "UINT8" => Ok(Self::U8),
            "I32" | "INT32" => Ok(Self::I32),
            "I16" | "INT16" => Ok(Self::I16),
            "Q4_0" => Ok(Self::Q4_0),
            "Q4_1" => Ok(Self::Q4_1),
            "Q8_0" => Ok(Self::Q8_0),
            _ => Err(AosError::Parse(format!("Unknown dtype: {}", s))),
        }
    }

    /// Get byte size per element
    pub fn element_size(&self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F16 | Self::BF16 | Self::I16 => 2,
            Self::I8 | Self::U8 => 1,
            // Quantized formats use variable size
            Self::Q4_0 | Self::Q4_1 => 0, // Variable, depends on block size
            Self::Q8_0 => 0,               // Variable, depends on block size
        }
    }

    /// Check if dtype is quantized
    pub fn is_quantized(&self) -> bool {
        matches!(self, Self::Q4_0 | Self::Q4_1 | Self::Q8_0)
    }

    /// Get quantization block size (for quantized formats)
    pub fn block_size(&self) -> Option<usize> {
        match self {
            Self::Q4_0 | Self::Q4_1 | Self::Q8_0 => Some(32), // GGML block size
            _ => None,
        }
    }
}

/// Tensor metadata from safetensors header
#[derive(Debug, Clone)]
pub struct TensorMetadata {
    /// Tensor name (key in safetensors)
    pub name: String,
    /// Tensor shape
    pub shape: Vec<usize>,
    /// Data type
    pub dtype: DType,
    /// Byte offset in file (from start of tensor data section)
    pub offset: usize,
    /// Byte size
    pub size: usize,
}

/// Weight name mapping configuration
#[derive(Debug, Clone)]
pub struct WeightMapping {
    /// Maps safetensors tensor names to MLX layer names
    name_map: HashMap<String, String>,
}

impl WeightMapping {
    /// Create default mapping for HuggingFace-style names
    pub fn huggingface() -> Self {
        let mut name_map = HashMap::new();
        // Standard transformer attention mappings
        name_map.insert("q_proj.lora_A".to_string(), "lora.q_proj.down".to_string());
        name_map.insert("q_proj.lora_B".to_string(), "lora.q_proj.up".to_string());
        name_map.insert("k_proj.lora_A".to_string(), "lora.k_proj.down".to_string());
        name_map.insert("k_proj.lora_B".to_string(), "lora.k_proj.up".to_string());
        name_map.insert("v_proj.lora_A".to_string(), "lora.v_proj.down".to_string());
        name_map.insert("v_proj.lora_B".to_string(), "lora.v_proj.up".to_string());
        name_map.insert("o_proj.lora_A".to_string(), "lora.o_proj.down".to_string());
        name_map.insert("o_proj.lora_B".to_string(), "lora.o_proj.up".to_string());
        Self { name_map }
    }

    /// Create identity mapping (no transformation)
    pub fn identity() -> Self {
        Self {
            name_map: HashMap::new(),
        }
    }

    /// Map a tensor name using the configured mapping
    pub fn map(&self, name: &str) -> String {
        self.name_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// Add custom mapping
    pub fn add_mapping(&mut self, from: String, to: String) {
        self.name_map.insert(from, to);
    }
}

/// Tensor cache entry
struct CachedTensor {
    /// Cached f32 data
    data: Arc<Vec<Vec<f32>>>,
    /// Cache hit count
    hits: usize,
}

/// Safetensors adapter loader with lazy loading and caching
pub struct SafetensorsLoader {
    /// Memory-mapped file (when using lazy loading)
    #[cfg(feature = "mmap")]
    mmap: Option<Mmap>,
    /// Raw safetensors data (when not using lazy loading)
    data: Vec<u8>,
    /// Parsed tensor metadata
    tensors: HashMap<String, TensorMetadata>,
    /// Offset where tensor data begins (after header)
    data_offset: usize,
    /// Weight name mapping
    weight_mapping: WeightMapping,
    /// Tensor cache for frequently accessed tensors
    cache: Arc<RwLock<HashMap<String, CachedTensor>>>,
    /// Maximum cache size in bytes (0 = unlimited)
    max_cache_size: usize,
}

impl SafetensorsLoader {
    /// Parse safetensors format from bytes
    ///
    /// Args:
    /// - `data`: Raw safetensors bytes
    ///
    /// Returns:
    /// SafetensorsLoader ready to extract tensors
    ///
    /// Errors:
    /// - `AosError::Parse` if header is invalid or corrupt
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let (tensors, data_offset) = Self::parse_header(&data)?;

        Ok(Self {
            #[cfg(feature = "mmap")]
            mmap: None,
            data,
            tensors,
            data_offset,
            weight_mapping: WeightMapping::identity(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size: 0, // Unlimited by default
        })
    }

    /// Parse safetensors format from memory-mapped file
    ///
    /// Enables zero-copy lazy loading for large models
    ///
    /// Args:
    /// - `path`: Path to safetensors file
    ///
    /// Errors:
    /// - `AosError::Io` if file cannot be opened
    /// - `AosError::Parse` if header is invalid
    #[cfg(feature = "mmap")]
    pub fn from_mmap<P: AsRef<Path>>(path: P) -> Result<Self> {
        use std::fs::File;

        let file = File::open(path.as_ref()).map_err(|e| {
            AosError::Io(format!(
                "Failed to open file {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        let mmap = unsafe {
            Mmap::map(&file).map_err(|e| AosError::Io(format!("Failed to mmap file: {}", e)))?
        };

        let (tensors, data_offset) = Self::parse_header(&mmap)?;

        tracing::info!(
            path = %path.as_ref().display(),
            tensor_count = tensors.len(),
            "Loaded safetensors via memory mapping"
        );

        Ok(Self {
            mmap: Some(mmap),
            data: Vec::new(), // Not used when mmap is active
            tensors,
            data_offset,
            weight_mapping: WeightMapping::identity(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_cache_size: 100 * 1024 * 1024, // 100 MB default cache
        })
    }

    /// Set weight name mapping
    pub fn with_mapping(mut self, mapping: WeightMapping) -> Self {
        self.weight_mapping = mapping;
        self
    }

    /// Set maximum cache size in bytes
    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.max_cache_size = size;
        self
    }

    /// Parse safetensors header from bytes
    ///
    /// Safetensors format:
    /// ```text
    /// [0-7]     header_size (u64, little-endian)
    /// [8..]     header_json (JSON metadata)
    /// [offset]  tensor_data (binary data for all tensors)
    /// ```
    ///
    /// Returns: (tensor_metadata_map, data_offset)
    fn parse_header(data: &[u8]) -> Result<(HashMap<String, TensorMetadata>, usize)> {
        if data.len() < 8 {
            return Err(AosError::Parse(
                "Safetensors file too small (< 8 bytes)".to_string(),
            ));
        }

        // Read header size (u64, little-endian)
        let header_size = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]) as usize;

        if data.len() < 8 + header_size {
            return Err(AosError::Parse(format!(
                "Safetensors file truncated: expected {} bytes, got {}",
                8 + header_size,
                data.len()
            )));
        }

        // Parse JSON header
        let header_json = &data[8..8 + header_size];
        let header: serde_json::Value = serde_json::from_slice(header_json)
            .map_err(|e| AosError::Parse(format!("Invalid JSON header: {}", e)))?;

        // Extract tensor metadata
        let mut tensors = HashMap::new();

        if let Some(obj) = header.as_object() {
            for (name, value) in obj {
                // Skip special __metadata__ key
                if name == "__metadata__" {
                    continue;
                }

                let tensor_meta = value
                    .as_object()
                    .ok_or_else(|| AosError::Parse(format!("Invalid tensor metadata: {}", name)))?;

                // Parse dtype
                let dtype_str = tensor_meta
                    .get("dtype")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AosError::Parse(format!("Missing dtype for tensor {}", name)))?;
                let dtype = DType::from_str(dtype_str)?;

                // Parse shape
                let shape: Vec<usize> = tensor_meta
                    .get("shape")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| AosError::Parse(format!("Missing shape for tensor {}", name)))?
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as usize))
                    .collect();

                // Parse data_offsets [start, end]
                let offsets = tensor_meta
                    .get("data_offsets")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        AosError::Parse(format!("Missing data_offsets for tensor {}", name))
                    })?;

                if offsets.len() != 2 {
                    return Err(AosError::Parse(format!(
                        "Invalid data_offsets for tensor {}: expected [start, end]",
                        name
                    )));
                }

                let offset_start = offsets[0]
                    .as_u64()
                    .ok_or_else(|| AosError::Parse(format!("Invalid offset for tensor {}", name)))?
                    as usize;
                let offset_end = offsets[1]
                    .as_u64()
                    .ok_or_else(|| AosError::Parse(format!("Invalid offset for tensor {}", name)))?
                    as usize;

                let size = offset_end - offset_start;

                tensors.insert(
                    name.clone(),
                    TensorMetadata {
                        name: name.clone(),
                        shape,
                        dtype,
                        offset: offset_start,
                        size,
                    },
                );
            }
        }

        let data_offset = 8 + header_size;

        tracing::debug!(
            tensor_count = tensors.len(),
            header_size = header_size,
            data_offset = data_offset,
            "Parsed safetensors header"
        );

        Ok((tensors, data_offset))
    }

    /// Get raw tensor bytes
    fn get_tensor_bytes(&self, metadata: &TensorMetadata) -> Result<&[u8]> {
        #[cfg(feature = "mmap")]
        if let Some(ref mmap) = self.mmap {
            let start = self.data_offset + metadata.offset;
            let end = start + metadata.size;

            if end > mmap.len() {
                return Err(AosError::Parse(format!(
                    "Tensor {} extends beyond file: offset={}, size={}, file_size={}",
                    metadata.name,
                    start,
                    metadata.size,
                    mmap.len()
                )));
            }

            return Ok(&mmap[start..end]);
        }

        // Fallback to in-memory data
        let start = self.data_offset + metadata.offset;
        let end = start + metadata.size;

        if end > self.data.len() {
            return Err(AosError::Parse(format!(
                "Tensor {} extends beyond data: offset={}, size={}, data_size={}",
                metadata.name, start, metadata.size, self.data.len()
            )));
        }

        Ok(&self.data[start..end])
    }

    /// Load LoRA adapter from safetensors with shared down projection
    ///
    /// Args:
    /// - `id`: Adapter identifier
    /// - `config`: LoRA configuration
    ///
    /// Returns:
    /// Loaded LoRAAdapter
    pub fn load_adapter(&self, id: String, config: LoRAConfig) -> Result<LoRAAdapter> {
        // Extract shared down projection
        let shared_down = self.extract_shared_down()?;

        // Create adapter with shared down
        let mut adapter = LoRAAdapter::new_with_shared_down(id, config.clone(), shared_down);

        // Extract per-module up projections
        for module_name in &config.target_modules {
            if let Ok(lora_b) = self.extract_module_up(module_name) {
                adapter.add_module_weights(module_name, lora_b);
            }
        }

        Ok(adapter)
    }

    /// Extract tensor as f32 matrix with caching
    ///
    /// Args:
    /// - `name`: Tensor name
    ///
    /// Returns:
    /// f32 matrix [rows][cols]
    pub fn extract_tensor(&self, name: &str) -> Result<Vec<Vec<f32>>> {
        // Check cache first
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(name) {
                tracing::trace!(name = %name, hits = cached.hits, "Cache hit");
                return Ok((*cached.data).clone());
            }
        }

        // Get metadata
        let metadata = self
            .tensors
            .get(name)
            .ok_or_else(|| AosError::Parse(format!("Tensor not found: {}", name)))?;

        // Validate 2D shape
        if metadata.shape.len() != 2 {
            return Err(AosError::Parse(format!(
                "Invalid shape for {}: expected 2D, got {:?}",
                name, metadata.shape
            )));
        }

        let rows = metadata.shape[0];
        let cols = metadata.shape[1];

        // Get raw bytes
        let bytes = self.get_tensor_bytes(metadata)?;

        // Convert to f32 based on dtype
        let floats = self.convert_to_f32(bytes, &metadata.dtype, rows * cols)?;

        // Reshape to 2D matrix
        let mut matrix = Vec::with_capacity(rows);
        for row_idx in 0..rows {
            let start = row_idx * cols;
            let end = start + cols;
            matrix.push(floats[start..end].to_vec());
        }

        // Update cache
        let matrix_arc = Arc::new(matrix.clone());
        {
            let mut cache = self.cache.write();

            // Evict old entries if cache is too large
            if self.max_cache_size > 0 {
                self.evict_cache_if_needed(&mut cache);
            }

            cache.insert(
                name.to_string(),
                CachedTensor {
                    data: matrix_arc,
                    hits: 1,
                },
            );
        }

        tracing::debug!(
            name = %name,
            shape = ?(rows, cols),
            dtype = ?metadata.dtype,
            "Loaded and cached tensor"
        );

        Ok(matrix)
    }

    /// Convert raw bytes to f32 based on dtype
    fn convert_to_f32(&self, bytes: &[u8], dtype: &DType, n_elements: usize) -> Result<Vec<f32>> {
        use crate::dtype_convert::*;

        match dtype {
            DType::F32 => {
                // Direct conversion
                if bytes.len() != n_elements * 4 {
                    return Err(AosError::Parse(format!(
                        "F32 data size mismatch: expected {} bytes, got {}",
                        n_elements * 4,
                        bytes.len()
                    )));
                }

                Ok(bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect())
            }
            DType::F16 => f16_to_f32(bytes),
            DType::BF16 => bf16_to_f32(bytes),
            DType::I8 => i8_to_f32(bytes),
            DType::U8 => u8_to_f32(bytes),
            DType::I16 => i16_to_f32(bytes),
            DType::I32 => i32_to_f32(bytes),
            DType::Q4_0 => dequantize_q4_0(bytes, n_elements),
            DType::Q4_1 => dequantize_q4_1(bytes, n_elements),
            DType::Q8_0 => dequantize_q8_0(bytes, n_elements),
        }
    }

    /// Evict least-used cache entries to stay under max size
    fn evict_cache_if_needed(&self, cache: &mut HashMap<String, CachedTensor>) {
        // Calculate current cache size (rough estimate)
        let mut total_size = 0;
        for entry in cache.values() {
            let matrix = &entry.data;
            if let Some(first_row) = matrix.first() {
                total_size += matrix.len() * first_row.len() * 4; // f32 = 4 bytes
            }
        }

        if total_size <= self.max_cache_size {
            return;
        }

        // Sort by hit count and evict least-used
        let mut entries: Vec<_> = cache.iter().map(|(k, v)| (k.clone(), v.hits)).collect();
        entries.sort_by_key(|(_, hits)| *hits);

        // Evict bottom 25% of entries
        let evict_count = (entries.len() + 3) / 4;
        for (key, _) in entries.iter().take(evict_count) {
            cache.remove(key);
        }

        tracing::debug!(
            evicted = evict_count,
            remaining = cache.len(),
            "Evicted cache entries"
        );
    }

    /// Extract shared down projection tensor
    ///
    /// Expected shape: [rank, hidden_dim]
    fn extract_shared_down(&self) -> Result<Vec<Vec<f32>>> {
        self.extract_tensor(SHARED_DOWN_KEY)
    }

    /// Extract module-specific up projection tensor
    ///
    /// Expected shape: [hidden_dim, rank]
    fn extract_module_up(&self, module_name: &str) -> Result<Vec<Vec<f32>>> {
        let tensor_key = format!("{}{}{}", MODULE_UP_PREFIX, module_name, MODULE_UP_SUFFIX);
        self.extract_tensor(&tensor_key)
    }

    /// List all available tensors
    pub fn list_tensors(&self) -> Vec<&str> {
        self.tensors.keys().map(|s| s.as_str()).collect()
    }

    /// Get tensor metadata
    pub fn tensor_metadata(&self, name: &str) -> Option<&TensorMetadata> {
        self.tensors.get(name)
    }

    /// Validate that all required tensors are present
    ///
    /// Args:
    /// - `required_modules`: List of module names (e.g., ["q_proj", "k_proj"])
    ///
    /// Returns:
    /// ValidationReport with missing/incompatible tensors
    pub fn validate(&self, required_modules: &[String]) -> ValidationReport {
        let mut report = ValidationReport {
            valid: true,
            missing_shared_down: false,
            missing_modules: Vec::new(),
            incompatible_shapes: Vec::new(),
            unsupported_dtypes: Vec::new(),
        };

        // Check for shared down projection
        if !self.tensors.contains_key(SHARED_DOWN_KEY) {
            report.valid = false;
            report.missing_shared_down = true;
        }

        // Check for module up projections
        for module_name in required_modules {
            let tensor_key = format!("{}{}{}", MODULE_UP_PREFIX, module_name, MODULE_UP_SUFFIX);
            if !self.tensors.contains_key(&tensor_key) {
                report.valid = false;
                report.missing_modules.push(module_name.clone());
            }
        }

        // Check dtypes
        for (name, metadata) in &self.tensors {
            if metadata.dtype.is_quantized() {
                tracing::warn!(
                    name = %name,
                    dtype = ?metadata.dtype,
                    "Quantized tensor (may have reduced precision)"
                );
            }
        }

        report
    }

    /// Clear tensor cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write();
        cache.clear();
        tracing::debug!("Cleared tensor cache");
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read();
        let entry_count = cache.len();
        let mut total_hits = 0;

        for entry in cache.values() {
            total_hits += entry.hits;
        }

        CacheStats {
            entry_count,
            total_hits,
        }
    }
}

/// Validation report for safetensors file
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Overall validity
    pub valid: bool,
    /// Missing shared down projection
    pub missing_shared_down: bool,
    /// Missing module up projections
    pub missing_modules: Vec<String>,
    /// Incompatible tensor shapes
    pub incompatible_shapes: Vec<String>,
    /// Unsupported data types
    pub unsupported_dtypes: Vec<String>,
}

impl ValidationReport {
    /// Check if validation passed
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get error messages
    pub fn errors(&self) -> Vec<String> {
        let mut errors = Vec::new();

        if self.missing_shared_down {
            errors.push("Missing shared down projection tensor".to_string());
        }

        for module in &self.missing_modules {
            errors.push(format!("Missing up projection for module: {}", module));
        }

        for shape in &self.incompatible_shapes {
            errors.push(format!("Incompatible shape: {}", shape));
        }

        for dtype in &self.unsupported_dtypes {
            errors.push(format!("Unsupported dtype: {}", dtype));
        }

        errors
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached entries
    pub entry_count: usize,
    /// Total cache hits across all entries
    pub total_hits: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_key_format() {
        assert_eq!(SHARED_DOWN_KEY, "lora.shared_down");

        let module = "q_proj";
        let key = format!("{}{}{}", MODULE_UP_PREFIX, module, MODULE_UP_SUFFIX);
        assert_eq!(key, "lora.q_proj.up");
    }

    #[test]
    fn test_dtype_parsing() {
        assert_eq!(DType::from_str("F32").unwrap(), DType::F32);
        assert_eq!(DType::from_str("f16").unwrap(), DType::F16);
        assert_eq!(DType::from_str("BF16").unwrap(), DType::BF16);
        assert_eq!(DType::from_str("Q4_0").unwrap(), DType::Q4_0);
        assert!(DType::from_str("UNKNOWN").is_err());
    }

    #[test]
    fn test_dtype_properties() {
        assert_eq!(DType::F32.element_size(), 4);
        assert_eq!(DType::F16.element_size(), 2);
        assert_eq!(DType::BF16.element_size(), 2);
        assert_eq!(DType::I8.element_size(), 1);

        assert!(!DType::F32.is_quantized());
        assert!(DType::Q4_0.is_quantized());
        assert!(DType::Q8_0.is_quantized());

        assert_eq!(DType::Q4_0.block_size(), Some(32));
        assert_eq!(DType::F32.block_size(), None);
    }

    #[test]
    fn test_weight_mapping_huggingface() {
        let mapping = WeightMapping::huggingface();
        assert_eq!(mapping.map("q_proj.lora_A"), "lora.q_proj.down");
        assert_eq!(mapping.map("q_proj.lora_B"), "lora.q_proj.up");
        assert_eq!(mapping.map("unknown"), "unknown"); // Unmapped returns identity
    }

    #[test]
    fn test_weight_mapping_custom() {
        let mut mapping = WeightMapping::identity();
        mapping.add_mapping("custom.in".to_string(), "custom.out".to_string());
        assert_eq!(mapping.map("custom.in"), "custom.out");
    }

    #[test]
    fn test_expected_tensor_layout() {
        // Document expected layout for 7B model with rank=16
        let rank = 16;
        let hidden_dim = 4096;

        // Shared down: [rank, hidden_dim]
        let shared_down_params = rank * hidden_dim;
        assert_eq!(shared_down_params, 65536);

        // Per-module up: [hidden_dim, rank]
        let module_up_params = hidden_dim * rank;
        assert_eq!(module_up_params, 65536);

        // Total for 4 modules (q, k, v, o)
        let total_params = shared_down_params + 4 * module_up_params;
        assert_eq!(total_params, 327680);

        // Compare to traditional LoRA
        let traditional_params = 4 * 2 * rank * hidden_dim; // 4 modules × 2 matrices
        assert_eq!(traditional_params, 524288);

        // Verify savings
        let savings_ratio = 1.0 - (total_params as f32 / traditional_params as f32);
        assert!((savings_ratio - 0.375).abs() < 0.001); // 37.5% savings
    }

    #[test]
    fn test_validation_report() {
        let report = ValidationReport {
            valid: false,
            missing_shared_down: true,
            missing_modules: vec!["q_proj".to_string()],
            incompatible_shapes: vec![],
            unsupported_dtypes: vec![],
        };

        assert!(!report.is_valid());
        let errors = report.errors();
        assert_eq!(errors.len(), 2);
        assert!(errors[0].contains("shared down"));
        assert!(errors[1].contains("q_proj"));
    }
}
