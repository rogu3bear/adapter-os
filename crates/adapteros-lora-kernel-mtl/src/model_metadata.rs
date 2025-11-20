//! Model Metadata Tracking for CoreML Conversions
//!
//! This module provides comprehensive metadata tracking for CoreML model conversions,
//! including version compatibility, conversion parameters, and performance hints.
//!
//! ## Metadata Components
//!
//! - **Model Info**: Architecture, dimensions, layer counts
//! - **Conversion Info**: Source format, quantization, timestamps
//! - **Performance Hints**: ANE compatibility, expected throughput
//! - **Version Info**: AdapterOS version, CoreML SDK version, macOS target
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_lora_kernel_mtl::model_metadata::{ModelMetadata, ModelInfo};
//!
//! let metadata = ModelMetadata::new(
//!     ModelInfo::qwen25_7b(),
//!     source_hash,
//!     quantization,
//! );
//!
//! metadata.save("model.metadata.json")?;
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

/// Complete model metadata for CoreML conversion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Model information
    pub model_info: ModelInfo,
    /// Conversion information
    pub conversion_info: ConversionInfo,
    /// Performance hints
    pub performance_hints: PerformanceHints,
    /// Version compatibility
    pub version_info: VersionInfo,
    /// Custom user-defined metadata
    #[serde(default)]
    pub custom_metadata: std::collections::HashMap<String, String>,
}

impl ModelMetadata {
    /// Create new model metadata
    pub fn new(
        model_info: ModelInfo,
        source_hash: B3Hash,
        quantization: Option<crate::conversion::QuantizationType>,
    ) -> Self {
        let conversion_info = ConversionInfo {
            source_format: SourceFormat::Safetensors,
            source_hash,
            coreml_hash: None,
            quantization: quantization.map(|q| format!("{:?}", q)),
            converted_at: chrono::Utc::now().to_rfc3339(),
            conversion_duration_secs: None,
        };

        let performance_hints = PerformanceHints {
            ane_compatible: quantization
                .map(|q| q.is_ane_compatible())
                .unwrap_or(true),
            estimated_memory_mb: model_info.estimate_memory_mb(quantization),
            expected_throughput_tokens_per_sec: None,
            batch_size: 1,
            sequence_length: 128,
        };

        let version_info = VersionInfo {
            adapteros_version: env!("CARGO_PKG_VERSION").to_string(),
            coreml_sdk_version: "7.0+".to_string(),
            min_macos_version: "13.0".to_string(),
            schema_version: "1.0.0".to_string(),
        };

        Self {
            model_info,
            conversion_info,
            performance_hints,
            version_info,
            custom_metadata: std::collections::HashMap::new(),
        }
    }

    /// Save metadata to JSON file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AosError::Validation(format!("Failed to serialize metadata: {}", e))
        })?;

        std::fs::write(path, json).map_err(|e| {
            AosError::Io(format!("Failed to write metadata file: {}", e))
        })?;

        info!("Saved model metadata: {}", path.display());
        Ok(())
    }

    /// Load metadata from JSON file
    pub fn load(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!("Failed to read metadata file: {}", e))
        })?;

        let metadata: Self = serde_json::from_str(&json).map_err(|e| {
            AosError::Validation(format!("Invalid metadata format: {}", e))
        })?;

        debug!("Loaded model metadata: {}", path.display());
        Ok(metadata)
    }

    /// Validate metadata compatibility with current version
    pub fn validate_compatibility(&self) -> Result<()> {
        // Check schema version
        if self.version_info.schema_version != "1.0.0" {
            return Err(AosError::Validation(format!(
                "Incompatible schema version: {} (expected 1.0.0)",
                self.version_info.schema_version
            )));
        }

        // Check macOS version
        let current_macos = Self::get_current_macos_version()?;
        if current_macos < self.version_info.min_macos_version {
            return Err(AosError::Validation(format!(
                "Requires macOS {} or later (current: {})",
                self.version_info.min_macos_version, current_macos
            )));
        }

        Ok(())
    }

    /// Get current macOS version
    fn get_current_macos_version() -> Result<String> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            let output = Command::new("sw_vers")
                .arg("-productVersion")
                .output()
                .map_err(|e| AosError::Config(format!("Failed to get macOS version: {}", e)))?;

            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        }

        #[cfg(not(target_os = "macos"))]
        {
            Ok("0.0.0".to_string())
        }
    }

    /// Update CoreML hash after conversion
    pub fn set_coreml_hash(&mut self, hash: B3Hash) {
        self.conversion_info.coreml_hash = Some(hash);
    }

    /// Update conversion duration
    pub fn set_conversion_duration(&mut self, duration_secs: f64) {
        self.conversion_info.conversion_duration_secs = Some(duration_secs);
    }

    /// Update expected throughput
    pub fn set_expected_throughput(&mut self, tokens_per_sec: f32) {
        self.performance_hints.expected_throughput_tokens_per_sec = Some(tokens_per_sec);
    }

    /// Add custom metadata field
    pub fn add_custom_field(&mut self, key: String, value: String) {
        self.custom_metadata.insert(key, value);
    }
}

/// Model architecture and dimension information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model name/identifier
    pub name: String,
    /// Architecture type (Qwen2.5, LLaMA, Mistral, etc.)
    pub architecture: String,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Hidden dimension
    pub hidden_size: usize,
    /// Number of transformer layers
    pub num_layers: usize,
    /// Number of attention heads
    pub num_attention_heads: usize,
    /// Intermediate FFN dimension
    pub intermediate_size: usize,
    /// Head dimension (hidden_size / num_attention_heads)
    pub head_dim: usize,
    /// Activation function (SwiGLU, GELU, etc.)
    pub activation: String,
    /// Normalization type (RMSNorm, LayerNorm)
    pub norm_type: String,
}

impl ModelInfo {
    /// Qwen2.5-7B model info
    pub fn qwen25_7b() -> Self {
        Self {
            name: "Qwen2.5-7B".to_string(),
            architecture: "Qwen2.5".to_string(),
            vocab_size: 152064,
            hidden_size: 3584,
            num_layers: 28,
            num_attention_heads: 28,
            intermediate_size: 18944,
            head_dim: 128,
            activation: "SwiGLU".to_string(),
            norm_type: "RMSNorm".to_string(),
        }
    }

    /// Qwen2.5-14B model info
    pub fn qwen25_14b() -> Self {
        Self {
            name: "Qwen2.5-14B".to_string(),
            architecture: "Qwen2.5".to_string(),
            vocab_size: 152064,
            hidden_size: 5120,
            num_layers: 48,
            num_attention_heads: 40,
            intermediate_size: 13824,
            head_dim: 128,
            activation: "SwiGLU".to_string(),
            norm_type: "RMSNorm".to_string(),
        }
    }

    /// LLaMA 2-7B model info
    pub fn llama2_7b() -> Self {
        Self {
            name: "LLaMA-2-7B".to_string(),
            architecture: "LLaMA".to_string(),
            vocab_size: 32000,
            hidden_size: 4096,
            num_layers: 32,
            num_attention_heads: 32,
            intermediate_size: 11008,
            head_dim: 128,
            activation: "SwiGLU".to_string(),
            norm_type: "RMSNorm".to_string(),
        }
    }

    /// Estimate memory usage in MB
    pub fn estimate_memory_mb(
        &self,
        quantization: Option<crate::conversion::QuantizationType>,
    ) -> f32 {
        // Estimate parameter count
        let embedding_params = self.vocab_size * self.hidden_size;
        let layer_params = self.num_layers
            * (
                // Attention: Q, K, V, O
                4 * self.hidden_size * self.hidden_size
                // FFN: Gate, Up, Down
                + 3 * self.hidden_size * self.intermediate_size
                // LayerNorm: 2 per layer
                + 2 * self.hidden_size
            );
        let output_params = self.vocab_size * self.hidden_size;

        let total_params = embedding_params + layer_params + output_params;

        // Bytes per parameter
        let bytes_per_param = match quantization {
            None | Some(crate::conversion::QuantizationType::Float32) => 4.0,
            Some(crate::conversion::QuantizationType::Float16) => 2.0,
            Some(crate::conversion::QuantizationType::Int8) => 1.0,
            Some(crate::conversion::QuantizationType::Int4) => 0.5,
        };

        let total_bytes = total_params as f32 * bytes_per_param;
        let total_mb = total_bytes / (1024.0 * 1024.0);

        total_mb
    }
}

/// Conversion information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionInfo {
    /// Source format (safetensors, PyTorch, ONNX)
    pub source_format: SourceFormat,
    /// Source file hash (BLAKE3)
    pub source_hash: B3Hash,
    /// CoreML model hash (BLAKE3)
    pub coreml_hash: Option<B3Hash>,
    /// Quantization applied
    pub quantization: Option<String>,
    /// Conversion timestamp (RFC3339)
    pub converted_at: String,
    /// Conversion duration in seconds
    pub conversion_duration_secs: Option<f64>,
}

/// Source format for conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceFormat {
    Safetensors,
    PyTorch,
    ONNX,
}

/// Performance hints for runtime optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceHints {
    /// Whether model is ANE-compatible
    pub ane_compatible: bool,
    /// Estimated memory usage (MB)
    pub estimated_memory_mb: f32,
    /// Expected throughput (tokens/sec on ANE)
    pub expected_throughput_tokens_per_sec: Option<f32>,
    /// Recommended batch size
    pub batch_size: usize,
    /// Recommended sequence length
    pub sequence_length: usize,
}

impl PerformanceHints {
    /// Check if model fits in available memory
    pub fn fits_in_memory(&self, available_mb: f32) -> bool {
        // Reserve 20% headroom
        let required_mb = self.estimated_memory_mb * 1.2;
        available_mb >= required_mb
    }

    /// Get recommended compute units
    pub fn recommended_compute_units(&self) -> &'static str {
        if self.ane_compatible {
            "ALL (CPU + GPU + ANE)"
        } else {
            "CPU_AND_GPU"
        }
    }
}

/// Version compatibility information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// AdapterOS version used for conversion
    pub adapteros_version: String,
    /// CoreML SDK version requirement
    pub coreml_sdk_version: String,
    /// Minimum macOS version
    pub min_macos_version: String,
    /// Metadata schema version
    pub schema_version: String,
}

impl VersionInfo {
    /// Check if current version is compatible
    pub fn is_compatible_with(&self, other: &VersionInfo) -> bool {
        self.schema_version == other.schema_version
            && self.min_macos_version <= other.min_macos_version
    }
}

/// Migration metadata for tracking model version history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationMetadata {
    /// Original metadata
    pub original: ModelMetadata,
    /// Current metadata
    pub current: ModelMetadata,
    /// Migration timestamp
    pub migrated_at: String,
    /// Migration notes
    pub notes: String,
}

impl MigrationMetadata {
    /// Create migration metadata
    pub fn new(original: ModelMetadata, current: ModelMetadata, notes: String) -> Self {
        Self {
            original,
            current,
            migrated_at: chrono::Utc::now().to_rfc3339(),
            notes,
        }
    }

    /// Save migration metadata
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            AosError::Validation(format!("Failed to serialize migration metadata: {}", e))
        })?;

        std::fs::write(path, json).map_err(|e| {
            AosError::Io(format!("Failed to write migration metadata: {}", e))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_qwen25() {
        let info = ModelInfo::qwen25_7b();
        assert_eq!(info.name, "Qwen2.5-7B");
        assert_eq!(info.vocab_size, 152064);
        assert_eq!(info.hidden_size, 3584);
        assert_eq!(info.num_layers, 28);
    }

    #[test]
    fn test_memory_estimation() {
        let info = ModelInfo::qwen25_7b();

        let fp32_mb = info.estimate_memory_mb(None);
        let fp16_mb = info.estimate_memory_mb(Some(crate::conversion::QuantizationType::Float16));
        let int8_mb = info.estimate_memory_mb(Some(crate::conversion::QuantizationType::Int8));

        // FP16 should be ~2x smaller than FP32
        assert!((fp16_mb * 2.0 - fp32_mb).abs() < fp32_mb * 0.1);

        // INT8 should be ~4x smaller than FP32
        assert!((int8_mb * 4.0 - fp32_mb).abs() < fp32_mb * 0.1);
    }

    #[test]
    fn test_metadata_serialization() {
        let model_info = ModelInfo::qwen25_7b();
        let source_hash = B3Hash::hash(b"test");
        let metadata = ModelMetadata::new(
            model_info,
            source_hash,
            Some(crate::conversion::QuantizationType::Float16),
        );

        let json = serde_json::to_string_pretty(&metadata).unwrap();
        let deserialized: ModelMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata.model_info.name, deserialized.model_info.name);
        assert_eq!(
            metadata.conversion_info.source_hash,
            deserialized.conversion_info.source_hash
        );
    }

    #[test]
    fn test_performance_hints() {
        let hints = PerformanceHints {
            ane_compatible: true,
            estimated_memory_mb: 1000.0,
            expected_throughput_tokens_per_sec: Some(50.0),
            batch_size: 1,
            sequence_length: 128,
        };

        assert!(hints.fits_in_memory(1500.0));
        assert!(!hints.fits_in_memory(1000.0)); // Needs 20% headroom
        assert_eq!(hints.recommended_compute_units(), "ALL (CPU + GPU + ANE)");
    }

    #[test]
    fn test_version_compatibility() {
        let v1 = VersionInfo {
            adapteros_version: "0.1.0".to_string(),
            coreml_sdk_version: "7.0".to_string(),
            min_macos_version: "13.0".to_string(),
            schema_version: "1.0.0".to_string(),
        };

        let v2 = VersionInfo {
            adapteros_version: "0.2.0".to_string(),
            coreml_sdk_version: "7.0".to_string(),
            min_macos_version: "13.0".to_string(),
            schema_version: "1.0.0".to_string(),
        };

        assert!(v1.is_compatible_with(&v2));
    }
}
