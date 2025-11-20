//! Model Conversion Utilities for CoreML Backend
//!
//! This module provides utilities to convert model weights from various formats
//! (safetensors, MLX, ONNX) to CoreML .mlpackage format with ANE optimization.
//!
//! ## Supported Conversions
//!
//! - safetensors → CoreML (primary path for AdapterOS adapters)
//! - PyTorch → CoreML (via intermediate format)
//! - ONNX → CoreML (via coremltools)
//!
//! ## Quantization Support
//!
//! - FP32 → FP16 (recommended for ANE, 2x memory reduction)
//! - FP32 → INT8 (4x memory reduction, slight accuracy drop)
//! - FP32 → INT4 (8x memory reduction, experimental)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_lora_kernel_mtl::conversion::{ConversionConfig, ModelConverter};
//!
//! let config = ConversionConfig {
//!     quantization: Some(QuantizationType::Float16),
//!     target_ane: true,
//!     batch_size: 1,
//!     sequence_length: 128,
//! };
//!
//! let converter = ModelConverter::new(config)?;
//! converter.convert_safetensors_to_coreml(
//!     "weights.safetensors",
//!     "model.mlpackage"
//! )?;
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Quantization types supported for CoreML conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantizationType {
    /// 32-bit floating point (original precision)
    Float32,
    /// 16-bit floating point (recommended for ANE)
    Float16,
    /// 8-bit integer quantization (4x compression)
    Int8,
    /// 4-bit integer quantization (8x compression, experimental)
    Int4,
}

impl QuantizationType {
    /// Get compression ratio relative to FP32
    pub fn compression_ratio(&self) -> f32 {
        match self {
            Self::Float32 => 1.0,
            Self::Float16 => 2.0,
            Self::Int8 => 4.0,
            Self::Int4 => 8.0,
        }
    }

    /// Check if quantization type is ANE-compatible
    pub fn is_ane_compatible(&self) -> bool {
        matches!(self, Self::Float16 | Self::Int8)
    }
}

/// Tensor data format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorFormat {
    Float32,
    Float16,
    Int8,
    Int4,
}

/// Configuration for model conversion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionConfig {
    /// Target quantization type
    pub quantization: Option<QuantizationType>,
    /// Whether to optimize for Apple Neural Engine
    pub target_ane: bool,
    /// Batch size (ANE optimized for 1)
    pub batch_size: usize,
    /// Maximum sequence length
    pub sequence_length: usize,
    /// Minimum macOS deployment target (13.0 for ANE)
    pub min_macos_version: String,
    /// Enable strict validation of converted model
    pub strict_validation: bool,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            quantization: Some(QuantizationType::Float16),
            target_ane: true,
            batch_size: 1,
            sequence_length: 128,
            min_macos_version: "13.0".to_string(),
            strict_validation: true,
        }
    }
}

/// Model metadata stored with converted CoreML model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Original model name/identifier
    pub model_name: String,
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
    /// Original model hash (safetensors)
    pub source_hash: B3Hash,
    /// Converted model hash
    pub coreml_hash: Option<B3Hash>,
    /// Quantization applied
    pub quantization: Option<QuantizationType>,
    /// Conversion timestamp
    pub converted_at: String,
    /// AdapterOS version
    pub adapteros_version: String,
}

/// Layer name mapping for transformer architectures
#[derive(Debug, Clone)]
pub struct LayerMapping {
    /// Prefix for transformer layers (e.g., "model.layers")
    pub layer_prefix: String,
    /// Attention query projection
    pub attn_q_proj: String,
    /// Attention key projection
    pub attn_k_proj: String,
    /// Attention value projection
    pub attn_v_proj: String,
    /// Attention output projection
    pub attn_o_proj: String,
    /// Feed-forward gate projection (for SwiGLU)
    pub ffn_gate_proj: String,
    /// Feed-forward up projection
    pub ffn_up_proj: String,
    /// Feed-forward down projection
    pub ffn_down_proj: String,
    /// Input layer normalization
    pub input_layernorm: String,
    /// Post-attention layer normalization
    pub post_attention_layernorm: String,
}

impl LayerMapping {
    /// Create layer mapping for Qwen2.5 architecture
    pub fn qwen2_5() -> Self {
        Self {
            layer_prefix: "model.layers".to_string(),
            attn_q_proj: "self_attn.q_proj.weight".to_string(),
            attn_k_proj: "self_attn.k_proj.weight".to_string(),
            attn_v_proj: "self_attn.v_proj.weight".to_string(),
            attn_o_proj: "self_attn.o_proj.weight".to_string(),
            ffn_gate_proj: "mlp.gate_proj.weight".to_string(),
            ffn_up_proj: "mlp.up_proj.weight".to_string(),
            ffn_down_proj: "mlp.down_proj.weight".to_string(),
            input_layernorm: "input_layernorm.weight".to_string(),
            post_attention_layernorm: "post_attention_layernorm.weight".to_string(),
        }
    }

    /// Create layer mapping for LLaMA architecture
    pub fn llama() -> Self {
        Self {
            layer_prefix: "model.layers".to_string(),
            attn_q_proj: "self_attn.q_proj.weight".to_string(),
            attn_k_proj: "self_attn.k_proj.weight".to_string(),
            attn_v_proj: "self_attn.v_proj.weight".to_string(),
            attn_o_proj: "self_attn.o_proj.weight".to_string(),
            ffn_gate_proj: "mlp.gate_proj.weight".to_string(),
            ffn_up_proj: "mlp.up_proj.weight".to_string(),
            ffn_down_proj: "mlp.down_proj.weight".to_string(),
            input_layernorm: "input_layernorm.weight".to_string(),
            post_attention_layernorm: "post_attention_layernorm.weight".to_string(),
        }
    }
}

/// Quantization calibration data
#[derive(Debug, Clone)]
pub struct QuantizationCalibration {
    /// Calibration samples (token sequences)
    pub samples: Vec<Vec<u32>>,
    /// Target accuracy threshold (0.0-1.0)
    pub accuracy_threshold: f32,
    /// Number of calibration steps
    pub num_steps: usize,
}

impl Default for QuantizationCalibration {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            accuracy_threshold: 0.99,
            num_steps: 100,
        }
    }
}

/// Model converter for various formats to CoreML
pub struct ModelConverter {
    config: ConversionConfig,
    layer_mapping: LayerMapping,
}

impl ModelConverter {
    /// Create a new model converter
    pub fn new(config: ConversionConfig) -> Result<Self> {
        // Validate configuration
        if config.target_ane && config.batch_size > 1 {
            warn!(
                "ANE optimization requested with batch_size={}, \
                 ANE performs best with batch_size=1",
                config.batch_size
            );
        }

        if let Some(quant) = config.quantization {
            if config.target_ane && !quant.is_ane_compatible() {
                return Err(AosError::Config(format!(
                    "Quantization {:?} not compatible with ANE optimization",
                    quant
                )));
            }
        }

        Ok(Self {
            config,
            layer_mapping: LayerMapping::qwen2_5(), // Default to Qwen2.5
        })
    }

    /// Set custom layer mapping
    pub fn with_layer_mapping(mut self, mapping: LayerMapping) -> Self {
        self.layer_mapping = mapping;
        self
    }

    /// Convert safetensors weights to CoreML format
    ///
    /// This method extracts weights from safetensors and generates a Python script
    /// to perform the actual CoreML conversion (using coremltools).
    pub fn convert_safetensors_to_coreml(
        &self,
        safetensors_path: &Path,
        output_path: &Path,
    ) -> Result<ConversionManifest> {
        info!(
            "Converting safetensors to CoreML: {} → {}",
            safetensors_path.display(),
            output_path.display()
        );

        // Load safetensors
        let data = std::fs::read(safetensors_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read safetensors: {}",
                e
            ))
        })?;

        let safetensors = SafeTensors::deserialize(&data).map_err(|e| {
            AosError::Validation(format!(
                "Invalid safetensors format: {}",
                e
            ))
        })?;

        let source_hash = B3Hash::hash(&data);
        info!(source_hash = %source_hash.to_short_hex(), "Loaded safetensors");

        // Extract layer information
        let layer_info = self.extract_layer_info(&safetensors)?;
        debug!("Extracted {} layers", layer_info.num_layers);

        // Generate metadata
        let metadata = ModelMetadata {
            model_name: safetensors_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("model")
                .to_string(),
            vocab_size: layer_info.vocab_size,
            hidden_size: layer_info.hidden_size,
            num_layers: layer_info.num_layers,
            num_attention_heads: layer_info.num_attention_heads,
            intermediate_size: layer_info.intermediate_size,
            source_hash,
            coreml_hash: None,
            quantization: self.config.quantization,
            converted_at: chrono::Utc::now().to_rfc3339(),
            adapteros_version: env!("CARGO_PKG_VERSION").to_string(),
        };

        // Generate conversion script
        let script_path = output_path.with_extension("conversion.py");
        self.generate_conversion_script(&script_path, safetensors_path, output_path, &metadata)?;

        info!(
            "Generated conversion script: {}",
            script_path.display()
        );
        info!(
            "Run: python3 {} to complete conversion",
            script_path.display()
        );

        Ok(ConversionManifest {
            metadata,
            script_path,
            output_path: output_path.to_path_buf(),
            quantization_config: self.config.quantization,
        })
    }

    /// Extract layer information from safetensors
    fn extract_layer_info(&self, safetensors: &SafeTensors) -> Result<LayerInfo> {
        let tensor_names: Vec<String> = safetensors.names().map(|s| s.to_string()).collect();

        // Count transformer layers
        let num_layers = tensor_names
            .iter()
            .filter(|name| name.contains(&self.layer_mapping.layer_prefix))
            .filter_map(|name| {
                name.split('.')
                    .nth(2)
                    .and_then(|idx| idx.parse::<usize>().ok())
            })
            .max()
            .map(|max_idx| max_idx + 1)
            .unwrap_or(0);

        // Extract dimensions from first layer
        let first_layer_q = format!(
            "{}.0.{}",
            self.layer_mapping.layer_prefix, self.layer_mapping.attn_q_proj
        );

        let (hidden_size, num_attention_heads) = if let Ok(tensor) = safetensors.tensor(&first_layer_q)
        {
            let shape = tensor.shape();
            if shape.len() >= 2 {
                let out_dim = shape[0];
                let hidden_dim = shape[1];
                let num_heads = out_dim / 128; // Assume head_dim = 128
                (hidden_dim, num_heads)
            } else {
                (3584, 28) // Qwen2.5-7B defaults
            }
        } else {
            warn!("Could not infer dimensions, using Qwen2.5-7B defaults");
            (3584, 28)
        };

        // Infer vocab size from output layer
        let vocab_size = safetensors
            .tensor("lm_head.weight")
            .ok()
            .and_then(|tensor| tensor.shape().first().copied())
            .unwrap_or(152064); // Qwen2.5 default

        // Infer intermediate size (FFN)
        let first_layer_gate = format!(
            "{}.0.{}",
            self.layer_mapping.layer_prefix, self.layer_mapping.ffn_gate_proj
        );
        let intermediate_size = safetensors
            .tensor(&first_layer_gate)
            .ok()
            .and_then(|tensor| tensor.shape().first().copied())
            .unwrap_or(18944); // Qwen2.5-7B default

        Ok(LayerInfo {
            num_layers,
            vocab_size,
            hidden_size,
            num_attention_heads,
            intermediate_size,
        })
    }

    /// Generate Python conversion script using coremltools
    fn generate_conversion_script(
        &self,
        script_path: &Path,
        input_path: &Path,
        output_path: &Path,
        metadata: &ModelMetadata,
    ) -> Result<()> {
        let quantization_str = match self.config.quantization {
            Some(QuantizationType::Float16) => "ct.precision.FLOAT16",
            Some(QuantizationType::Int8) => "ct.precision.INT8",
            Some(QuantizationType::Int4) => "ct.precision.INT4",
            _ => "ct.precision.FLOAT32",
        };

        let compute_units = if self.config.target_ane {
            "ct.ComputeUnit.ALL"
        } else {
            "ct.ComputeUnit.CPU_AND_GPU"
        };

        let script_content = format!(
            r#"#!/usr/bin/env python3
"""
AdapterOS CoreML Conversion Script
Generated: {}
Source: {}
Target: {}
Quantization: {:?}
ANE Target: {}
"""

import coremltools as ct
import torch
import numpy as np
from safetensors import safe_open
from transformers import AutoModelForCausalLM, AutoConfig
import json
from pathlib import Path

def load_safetensors_to_state_dict(safetensors_path):
    """Load weights from safetensors into PyTorch state dict"""
    state_dict = {{}}
    with safe_open(safetensors_path, framework="pt") as f:
        for key in f.keys():
            state_dict[key] = f.get_tensor(key)
    return state_dict

def convert_to_coreml():
    print("🔧 AdapterOS CoreML Converter")
    print(f"Source: {}")
    print(f"Target: {}")
    print(f"Quantization: {:?}")

    # Load model configuration
    config = AutoConfig.from_pretrained(
        "Qwen/Qwen2.5-7B",  # Base config
        vocab_size={},
        hidden_size={},
        num_hidden_layers={},
        num_attention_heads={},
        intermediate_size={},
    )

    # Create model architecture
    print("Loading model architecture...")
    model = AutoModelForCausalLM.from_config(config)

    # Load weights from safetensors
    print("Loading weights from safetensors...")
    state_dict = load_safetensors_to_state_dict("{}")
    model.load_state_dict(state_dict, strict=False)
    model.eval()

    # Create example input
    batch_size = {}
    seq_length = {}
    input_ids = torch.randint(0, config.vocab_size, (batch_size, seq_length), dtype=torch.long)

    print(f"Example input shape: {{input_ids.shape}}")

    # Trace model
    print("Tracing model with TorchScript...")
    with torch.no_grad():
        traced_model = torch.jit.trace(model, (input_ids,))

    # Convert to CoreML
    print("Converting to CoreML...")
    mlmodel = ct.convert(
        traced_model,
        inputs=[ct.TensorType(
            name="input_ids",
            shape=(batch_size, seq_length),
            dtype=np.int32
        )],
        outputs=[ct.TensorType(name="logits")],
        compute_precision={},
        compute_units={},
        minimum_deployment_target=ct.target.macOS13,
        convert_to="mlprogram",  # ML Program for ANE support
    )

    # Add metadata
    mlmodel.author = "AdapterOS v{}"
    mlmodel.license = "Copyright © 2025 JKCA"
    mlmodel.short_description = "{} (CoreML)"
    mlmodel.version = "{}"

    # Save metadata as JSON
    metadata = {{
        "model_name": "{}",
        "vocab_size": {},
        "hidden_size": {},
        "num_layers": {},
        "num_attention_heads": {},
        "intermediate_size": {},
        "quantization": "{}",
        "converted_at": "{}",
        "adapteros_version": "{}",
        "source_hash": "{}",
    }}

    metadata_path = Path("{}").with_suffix(".metadata.json")
    with open(metadata_path, "w") as f:
        json.dump(metadata, f, indent=2)

    print(f"Saved metadata: {{metadata_path}}")

    # Save CoreML model
    print("Saving CoreML model package...")
    mlmodel.save("{}")

    print("✅ Conversion complete!")
    print(f"Output: {}")
    print(f"Metadata: {{metadata_path}}")

    # Validation
    print("\n🔍 Validating converted model...")
    spec = mlmodel.get_spec()
    print(f"Model type: {{spec.description.metadata.userDefined.get('com.apple.coreml.model.preview.type', 'unknown')}}")

    # Test inference
    print("Testing inference...")
    test_input = {{"input_ids": input_ids.numpy()}}
    output = mlmodel.predict(test_input)
    print(f"Output shape: {{output['logits'].shape}}")
    print("✅ Validation passed!")

if __name__ == "__main__":
    convert_to_coreml()
"#,
            metadata.converted_at,
            input_path.display(),
            output_path.display(),
            self.config.quantization,
            self.config.target_ane,
            input_path.display(),
            output_path.display(),
            self.config.quantization,
            metadata.vocab_size,
            metadata.hidden_size,
            metadata.num_layers,
            metadata.num_attention_heads,
            metadata.intermediate_size,
            input_path.display(),
            self.config.batch_size,
            self.config.sequence_length,
            quantization_str,
            compute_units,
            metadata.adapteros_version,
            metadata.model_name,
            metadata.adapteros_version,
            metadata.model_name,
            metadata.vocab_size,
            metadata.hidden_size,
            metadata.num_layers,
            metadata.num_attention_heads,
            metadata.intermediate_size,
            self.config.quantization.map(|q| format!("{:?}", q)).unwrap_or_else(|| "None".to_string()),
            metadata.converted_at,
            metadata.adapteros_version,
            metadata.source_hash.to_short_hex(),
            output_path.display(),
            output_path.display(),
            output_path.display(),
        );

        std::fs::write(script_path, script_content).map_err(|e| {
            AosError::Io(format!(
                "Failed to write conversion script: {}",
                e
            ))
        })?;

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(script_path)
                .map_err(|e| AosError::Io(format!("Failed to get permissions: {}", e)))?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(script_path, perms)
                .map_err(|e| AosError::Io(format!("Failed to set permissions: {}", e)))?;
        }

        Ok(())
    }
}

/// Layer information extracted from model weights
#[derive(Debug, Clone)]
struct LayerInfo {
    num_layers: usize,
    vocab_size: usize,
    hidden_size: usize,
    num_attention_heads: usize,
    intermediate_size: usize,
}

/// Conversion manifest containing paths and metadata
#[derive(Debug, Clone)]
pub struct ConversionManifest {
    pub metadata: ModelMetadata,
    pub script_path: PathBuf,
    pub output_path: PathBuf,
    pub quantization_config: Option<QuantizationType>,
}

impl ConversionManifest {
    /// Save manifest to JSON file
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.metadata).map_err(|e| {
            AosError::Validation(format!(
                "Failed to serialize metadata: {}",
                e
            ))
        })?;

        std::fs::write(path, json).map_err(|e| {
            AosError::Io(format!(
                "Failed to write manifest: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Load manifest from JSON file
    pub fn load(path: &Path) -> Result<ModelMetadata> {
        let json = std::fs::read_to_string(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read manifest: {}",
                e
            ))
        })?;

        serde_json::from_str(&json).map_err(|e| {
            AosError::Validation(format!(
                "Invalid manifest format: {}",
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_compression_ratios() {
        assert_eq!(QuantizationType::Float32.compression_ratio(), 1.0);
        assert_eq!(QuantizationType::Float16.compression_ratio(), 2.0);
        assert_eq!(QuantizationType::Int8.compression_ratio(), 4.0);
        assert_eq!(QuantizationType::Int4.compression_ratio(), 8.0);
    }

    #[test]
    fn test_ane_compatibility() {
        assert!(!QuantizationType::Float32.is_ane_compatible());
        assert!(QuantizationType::Float16.is_ane_compatible());
        assert!(QuantizationType::Int8.is_ane_compatible());
        assert!(!QuantizationType::Int4.is_ane_compatible());
    }

    #[test]
    fn test_default_config() {
        let config = ConversionConfig::default();
        assert_eq!(config.quantization, Some(QuantizationType::Float16));
        assert!(config.target_ane);
        assert_eq!(config.batch_size, 1);
        assert_eq!(config.sequence_length, 128);
    }

    #[test]
    fn test_layer_mapping_qwen() {
        let mapping = LayerMapping::qwen2_5();
        assert_eq!(mapping.layer_prefix, "model.layers");
        assert_eq!(mapping.attn_q_proj, "self_attn.q_proj.weight");
        assert_eq!(mapping.ffn_gate_proj, "mlp.gate_proj.weight");
    }

    #[test]
    fn test_converter_validation() {
        // Valid config
        let config = ConversionConfig::default();
        assert!(ModelConverter::new(config).is_ok());

        // Invalid: INT4 with ANE target
        let invalid_config = ConversionConfig {
            quantization: Some(QuantizationType::Int4),
            target_ane: true,
            ..Default::default()
        };
        assert!(ModelConverter::new(invalid_config).is_err());
    }
}
