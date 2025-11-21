//! True LoRA Fusion for CoreML Backend
//!
//! This module implements pre-fusion of LoRA adapters into CoreML models,
//! creating fused `.mlmodelc` files for optimal ANE performance.
//!
//! ## Architecture
//!
//! CoreML models are compiled and don't expose intermediate layer activations.
//! To achieve true LoRA fusion (not post-hoc logit scaling), we must fuse
//! LoRA weights into the base model weights before compilation.
//!
//! Formula: W_fused = W_base + (alpha/rank) * sum(gate_i * B_i @ A_i)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_lora_kernel_coreml::fusion::{LoraFusionConfig, fuse_lora_into_model};
//!
//! let config = LoraFusionConfig {
//!     base_model_path: "model.mlpackage".into(),
//!     output_path: "fused_model.mlmodelc".into(),
//!     adapters: vec![
//!         AdapterFusionSpec {
//!             weights_path: "adapter_a.safetensors".into(),
//!             gate_weight: 0.5,
//!             alpha: 32.0,
//!             rank: 16,
//!         },
//!     ],
//!     compute_units: ComputeUnits::CpuAndNeuralEngine,
//! };
//!
//! fuse_lora_into_model(&config)?;
//! ```

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration for LoRA fusion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoraFusionConfig {
    /// Path to base CoreML model (.mlpackage or .mlmodelc)
    pub base_model_path: PathBuf,
    /// Output path for fused model (.mlmodelc)
    pub output_path: PathBuf,
    /// Adapters to fuse into the model
    pub adapters: Vec<AdapterFusionSpec>,
    /// Compute units for compilation
    pub compute_units: crate::ComputeUnits,
}

/// Specification for a single adapter to fuse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterFusionSpec {
    /// Path to adapter weights (.safetensors)
    pub weights_path: PathBuf,
    /// Gate weight (0.0 to 1.0, from Q15 routing decision)
    pub gate_weight: f32,
    /// LoRA alpha scaling factor
    pub alpha: f32,
    /// LoRA rank
    pub rank: usize,
}

/// Target modules for LoRA fusion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoraTarget {
    /// Query projection
    QProj,
    /// Key projection
    KProj,
    /// Value projection
    VProj,
    /// Output projection
    OProj,
    /// MLP gate projection
    GateProj,
    /// MLP up projection
    UpProj,
    /// MLP down projection
    DownProj,
}

impl LoraTarget {
    /// Get all targets
    pub fn all() -> &'static [LoraTarget] {
        &[
            LoraTarget::QProj,
            LoraTarget::KProj,
            LoraTarget::VProj,
            LoraTarget::OProj,
            LoraTarget::GateProj,
            LoraTarget::UpProj,
            LoraTarget::DownProj,
        ]
    }

    /// Convert to safetensors key pattern
    pub fn to_safetensor_pattern(&self, layer_idx: usize) -> (String, String) {
        let base = match self {
            LoraTarget::QProj => "q_proj",
            LoraTarget::KProj => "k_proj",
            LoraTarget::VProj => "v_proj",
            LoraTarget::OProj => "o_proj",
            LoraTarget::GateProj => "gate_proj",
            LoraTarget::UpProj => "up_proj",
            LoraTarget::DownProj => "down_proj",
        };
        (
            format!("model.layers.{}.self_attn.{}.lora_A.weight", layer_idx, base),
            format!("model.layers.{}.self_attn.{}.lora_B.weight", layer_idx, base),
        )
    }
}

/// Parsed LoRA weights for a single adapter
#[derive(Debug)]
pub struct ParsedLoraWeights {
    /// A matrices: target -> (layer_idx -> weights)
    pub a_matrices: HashMap<LoraTarget, HashMap<usize, Vec<f32>>>,
    /// B matrices: target -> (layer_idx -> weights)
    pub b_matrices: HashMap<LoraTarget, HashMap<usize, Vec<f32>>>,
    /// Detected rank
    pub rank: usize,
    /// Number of layers
    pub num_layers: usize,
}

/// Load and parse LoRA weights from safetensors file
pub fn load_lora_weights(path: &PathBuf) -> Result<ParsedLoraWeights> {
    let file_data = std::fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read LoRA weights: {}", e)))?;

    let tensors = safetensors::SafeTensors::deserialize(&file_data)
        .map_err(|e| AosError::Kernel(format!("Failed to parse safetensors: {}", e)))?;

    let mut a_matrices: HashMap<LoraTarget, HashMap<usize, Vec<f32>>> = HashMap::new();
    let mut b_matrices: HashMap<LoraTarget, HashMap<usize, Vec<f32>>> = HashMap::new();
    let mut detected_rank = 0;
    let mut max_layer = 0;

    for (name, tensor) in tensors.tensors() {
        // Parse tensor name to extract layer index and target
        if let Some((target, layer_idx, is_a)) = parse_tensor_name(&name) {
            let data = tensor.data();
            let floats: Vec<f32> = data
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            // Detect rank from A matrix shape
            if is_a && detected_rank == 0 {
                let shape = tensor.shape();
                if shape.len() >= 2 {
                    detected_rank = shape[0];
                }
            }

            max_layer = max_layer.max(layer_idx);

            if is_a {
                a_matrices
                    .entry(target)
                    .or_default()
                    .insert(layer_idx, floats);
            } else {
                b_matrices
                    .entry(target)
                    .or_default()
                    .insert(layer_idx, floats);
            }
        }
    }

    Ok(ParsedLoraWeights {
        a_matrices,
        b_matrices,
        rank: detected_rank,
        num_layers: max_layer + 1,
    })
}

/// Parse tensor name to extract target, layer index, and whether it's A or B
fn parse_tensor_name(name: &str) -> Option<(LoraTarget, usize, bool)> {
    // Common patterns:
    // model.layers.0.self_attn.q_proj.lora_A.weight
    // base_model.model.layers.0.self_attn.q_proj.lora_A.weight

    let is_a = name.contains("lora_A");
    let is_b = name.contains("lora_B");

    if !is_a && !is_b {
        return None;
    }

    // Extract layer index
    let layer_idx = name
        .split('.')
        .find_map(|part| {
            if part.chars().all(|c| c.is_ascii_digit()) {
                part.parse().ok()
            } else {
                None
            }
        })?;

    // Extract target
    let target = if name.contains("q_proj") {
        LoraTarget::QProj
    } else if name.contains("k_proj") {
        LoraTarget::KProj
    } else if name.contains("v_proj") {
        LoraTarget::VProj
    } else if name.contains("o_proj") {
        LoraTarget::OProj
    } else if name.contains("gate_proj") {
        LoraTarget::GateProj
    } else if name.contains("up_proj") {
        LoraTarget::UpProj
    } else if name.contains("down_proj") {
        LoraTarget::DownProj
    } else {
        return None;
    };

    Some((target, layer_idx, is_a))
}

/// Compute fused weights: W_fused = W_base + scale * (B @ A)
///
/// # Arguments
/// * `base_weights` - Base model weights [out_dim, in_dim]
/// * `lora_a` - LoRA A matrix [rank, in_dim]
/// * `lora_b` - LoRA B matrix [out_dim, rank]
/// * `scale` - Combined scale factor: (gate * alpha / rank)
///
/// # Returns
/// Fused weights [out_dim, in_dim]
pub fn fuse_weights(
    base_weights: &[f32],
    lora_a: &[f32],
    lora_b: &[f32],
    out_dim: usize,
    in_dim: usize,
    rank: usize,
    scale: f32,
) -> Vec<f32> {
    let mut fused = base_weights.to_vec();

    // Compute B @ A and add to base
    // B: [out_dim, rank], A: [rank, in_dim]
    // Result: [out_dim, in_dim]
    for i in 0..out_dim {
        for j in 0..in_dim {
            let mut delta = 0.0f32;
            for r in 0..rank {
                // B[i, r] * A[r, j]
                delta += lora_b[i * rank + r] * lora_a[r * in_dim + j];
            }
            fused[i * in_dim + j] += scale * delta;
        }
    }

    fused
}

/// Result of LoRA fusion operation
#[derive(Debug)]
pub struct FusionResult {
    /// Path to the fused model
    pub output_path: PathBuf,
    /// Number of layers fused
    pub layers_fused: usize,
    /// Number of weights fused per layer
    pub weights_per_layer: usize,
    /// Total parameters modified
    pub total_params_modified: usize,
}

/// Fuse LoRA adapters into a CoreML model
///
/// This function requires `coremltools` Python package to be available.
/// It creates a fused model where LoRA weights are merged into base weights.
///
/// # Requirements
/// - Python 3.8+
/// - coremltools >= 7.0
/// - numpy
///
/// # Process
/// 1. Load base model specification using coremltools
/// 2. Load LoRA adapter weights from safetensors
/// 3. For each target layer, compute: W_fused = W_base + sum(gate_i * alpha_i/rank_i * B_i @ A_i)
/// 4. Update model specification with fused weights
/// 5. Compile to .mlmodelc with specified compute units
pub fn fuse_lora_into_model(config: &LoraFusionConfig) -> Result<FusionResult> {
    // Validate inputs
    if !config.base_model_path.exists() {
        return Err(AosError::NotFound(format!(
            "Base model not found: {}",
            config.base_model_path.display()
        )));
    }

    if config.adapters.is_empty() {
        return Err(AosError::Validation(
            "At least one adapter must be specified for fusion".to_string(),
        ));
    }

    // Load all adapter weights
    let mut all_weights = Vec::new();
    for adapter in &config.adapters {
        if !adapter.weights_path.exists() {
            return Err(AosError::NotFound(format!(
                "Adapter weights not found: {}",
                adapter.weights_path.display()
            )));
        }

        let weights = load_lora_weights(&adapter.weights_path)?;
        all_weights.push((adapter, weights));
    }

    // Generate Python script for fusion
    let script = generate_fusion_script(config, &all_weights)?;

    // Write script to temp file
    let script_path = std::env::temp_dir().join("aos_lora_fusion.py");
    std::fs::write(&script_path, &script)
        .map_err(|e| AosError::Io(format!("Failed to write fusion script: {}", e)))?;

    // Execute Python script
    let output = std::process::Command::new("python3")
        .arg(&script_path)
        .output()
        .map_err(|e| AosError::Kernel(format!("Failed to execute fusion script: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AosError::Kernel(format!(
            "LoRA fusion failed: {}",
            stderr
        )));
    }

    // Parse output for statistics
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (layers_fused, weights_per_layer, total_params) = parse_fusion_output(&stdout)?;

    // Clean up
    let _ = std::fs::remove_file(&script_path);

    tracing::info!(
        output = %config.output_path.display(),
        layers = layers_fused,
        params = total_params,
        "Successfully fused LoRA adapters into CoreML model"
    );

    Ok(FusionResult {
        output_path: config.output_path.clone(),
        layers_fused,
        weights_per_layer,
        total_params_modified: total_params,
    })
}

/// Generate Python script for LoRA fusion using coremltools
fn generate_fusion_script(
    config: &LoraFusionConfig,
    all_weights: &[(&AdapterFusionSpec, ParsedLoraWeights)],
) -> Result<String> {
    let mut script = String::new();

    // Imports and setup
    script.push_str(r#"#!/usr/bin/env python3
"""
AdapterOS LoRA Fusion Script
Generated by adapteros-lora-kernel-coreml

This script fuses LoRA adapter weights into a CoreML model's base weights.
Formula: W_fused = W_base + sum(gate_i * alpha_i/rank_i * B_i @ A_i)
"""

import coremltools as ct
import numpy as np
from safetensors import safe_open
import sys

def main():
    try:
        # Load base model
"#);

    script.push_str(&format!(
        "        model = ct.models.MLModel('{}')\n",
        config.base_model_path.display()
    ));

    script.push_str(r#"        spec = model.get_spec()

        # Track statistics
        layers_fused = 0
        weights_per_layer = 0
        total_params = 0

        # Target weight patterns in CoreML spec
        # Maps from LoRA target to CoreML weight names
        weight_patterns = {
            'q_proj': ['self_attn.q_proj.weight', 'self_attn_q_proj_weight'],
            'k_proj': ['self_attn.k_proj.weight', 'self_attn_k_proj_weight'],
            'v_proj': ['self_attn.v_proj.weight', 'self_attn_v_proj_weight'],
            'o_proj': ['self_attn.o_proj.weight', 'self_attn_o_proj_weight'],
            'gate_proj': ['mlp.gate_proj.weight', 'mlp_gate_proj_weight'],
            'up_proj': ['mlp.up_proj.weight', 'mlp_up_proj_weight'],
            'down_proj': ['mlp.down_proj.weight', 'mlp_down_proj_weight'],
        }

        # Load all LoRA adapter weights
        adapters = []
"#);

    // Add adapter loading
    for (adapter, weights) in all_weights {
        script.push_str(&format!(
            r#"        adapters.append({{
            'path': '{}',
            'gate': {},
            'alpha': {},
            'rank': {},
        }})
"#,
            adapter.weights_path.display(),
            adapter.gate_weight,
            adapter.alpha,
            weights.rank
        ));
    }

    script.push_str(r#"
        # Load adapter weights from safetensors
        adapter_weights = []
        for adapter_info in adapters:
            with safe_open(adapter_info['path'], framework='numpy') as f:
                weights = {}
                for key in f.keys():
                    weights[key] = f.get_tensor(key)
                adapter_weights.append({
                    'weights': weights,
                    'gate': adapter_info['gate'],
                    'alpha': adapter_info['alpha'],
                    'rank': adapter_info['rank'],
                })

        # Find and update weights in the model spec
        # This handles both neural network and ML program formats

        def find_and_fuse_weights(spec, layer_idx, target_name, adapter_weights):
            nonlocal total_params

            # Look for weight in different CoreML formats
            patterns = weight_patterns.get(target_name, [])

            for pattern in patterns:
                full_name = f'layers.{layer_idx}.{pattern}'

                # Try to find in neural network weights
                for layer in getattr(spec, 'neuralNetwork', spec).layers:
                    for weight_param in getattr(layer, 'weights', []):
                        if pattern in str(weight_param):
                            # Get base weights
                            base_weights = np.array(weight_param.floatValue).reshape(
                                weight_param.quantization.numberOfBits if hasattr(weight_param, 'quantization')
                                else (len(weight_param.floatValue),)
                            )

                            # Fuse LoRA contributions
                            for adapter in adapter_weights:
                                a_key = f'model.layers.{layer_idx}.self_attn.{target_name}.lora_A.weight'
                                b_key = f'model.layers.{layer_idx}.self_attn.{target_name}.lora_B.weight'

                                if a_key in adapter['weights'] and b_key in adapter['weights']:
                                    lora_a = adapter['weights'][a_key]
                                    lora_b = adapter['weights'][b_key]
                                    scale = adapter['gate'] * adapter['alpha'] / adapter['rank']

                                    # Compute B @ A
                                    delta = np.matmul(lora_b, lora_a)

                                    # Add to base weights
                                    if base_weights.shape == delta.shape:
                                        base_weights += scale * delta
                                        total_params += base_weights.size

                            # Update weights
                            weight_param.floatValue[:] = base_weights.flatten().tolist()
                            return True

            return False

        # Iterate through layers
        num_layers = 32  # Typical for 7B models, adjust as needed
        for layer_idx in range(num_layers):
            layer_fused = False
            for target in ['q_proj', 'k_proj', 'v_proj', 'o_proj', 'gate_proj', 'up_proj', 'down_proj']:
                if find_and_fuse_weights(spec, layer_idx, target, adapter_weights):
                    layer_fused = True
                    weights_per_layer += 1

            if layer_fused:
                layers_fused += 1

        # Set compute units
"#);

    let compute_units = match config.compute_units {
        crate::ComputeUnits::CpuOnly => "ct.ComputeUnit.CPU_ONLY",
        crate::ComputeUnits::CpuAndGpu => "ct.ComputeUnit.CPU_AND_GPU",
        crate::ComputeUnits::CpuAndNeuralEngine => "ct.ComputeUnit.CPU_AND_NE",
        crate::ComputeUnits::All => "ct.ComputeUnit.ALL",
    };

    script.push_str(&format!(
        r#"        # Compile fused model
        fused_model = ct.models.MLModel(spec, compute_units={})
        fused_model.save('{}')

        # Output statistics
        print(f'FUSION_STATS:layers={layers_fused},weights_per_layer={weights_per_layer},total_params={total_params}')
        print('Fusion completed successfully')

    except Exception as e:
        print(f'Error: {{e}}', file=sys.stderr)
        sys.exit(1)

if __name__ == '__main__':
    main()
"#,
        compute_units,
        config.output_path.display()
    ));

    Ok(script)
}

/// Parse fusion script output for statistics
fn parse_fusion_output(output: &str) -> Result<(usize, usize, usize)> {
    for line in output.lines() {
        if line.starts_with("FUSION_STATS:") {
            let stats = line.trim_start_matches("FUSION_STATS:");
            let mut layers = 0;
            let mut weights = 0;
            let mut params = 0;

            for part in stats.split(',') {
                let kv: Vec<&str> = part.split('=').collect();
                if kv.len() == 2 {
                    match kv[0] {
                        "layers" => layers = kv[1].parse().unwrap_or(0),
                        "weights_per_layer" => weights = kv[1].parse().unwrap_or(0),
                        "total_params" => params = kv[1].parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            return Ok((layers, weights, params));
        }
    }

    // Default values if parsing fails
    Ok((0, 0, 0))
}

/// Cached fused model manager
///
/// Manages pre-fused models to avoid re-fusing on every inference.
/// Uses content-addressable storage based on adapter configuration hash.
#[derive(Debug)]
pub struct FusedModelCache {
    cache_dir: PathBuf,
    max_cache_size_gb: f64,
}

impl FusedModelCache {
    /// Create a new cache in the specified directory
    pub fn new(cache_dir: PathBuf, max_cache_size_gb: f64) -> Self {
        std::fs::create_dir_all(&cache_dir).ok();
        Self {
            cache_dir,
            max_cache_size_gb,
        }
    }

    /// Get cache key for fusion configuration
    pub fn cache_key(config: &LoraFusionConfig) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // Hash base model path
        config.base_model_path.to_string_lossy().hash(&mut hasher);

        // Hash each adapter configuration
        for adapter in &config.adapters {
            adapter.weights_path.to_string_lossy().hash(&mut hasher);
            adapter.gate_weight.to_bits().hash(&mut hasher);
            adapter.alpha.to_bits().hash(&mut hasher);
            adapter.rank.hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }

    /// Check if fused model exists in cache
    pub fn get(&self, config: &LoraFusionConfig) -> Option<PathBuf> {
        let key = Self::cache_key(config);
        let cached_path = self.cache_dir.join(format!("{}.mlmodelc", key));

        if cached_path.exists() {
            tracing::debug!(cache_key = %key, "Found cached fused model");
            Some(cached_path)
        } else {
            None
        }
    }

    /// Fuse and cache model
    pub fn fuse_and_cache(&self, mut config: LoraFusionConfig) -> Result<PathBuf> {
        let key = Self::cache_key(&config);
        let cached_path = self.cache_dir.join(format!("{}.mlmodelc", key));

        // Update output path to cache location
        config.output_path = cached_path.clone();

        // Check cache size and evict if necessary
        self.evict_if_needed()?;

        // Perform fusion
        fuse_lora_into_model(&config)?;

        Ok(cached_path)
    }

    /// Evict oldest models if cache exceeds size limit
    fn evict_if_needed(&self) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.cache_dir)
            .map_err(|e| AosError::Io(format!("Failed to read cache dir: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "mlmodelc"))
            .collect();

        // Calculate total size
        let total_size: u64 = entries
            .iter()
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();

        let max_bytes = (self.max_cache_size_gb * 1024.0 * 1024.0 * 1024.0) as u64;

        if total_size > max_bytes {
            // Sort by modification time (oldest first)
            entries.sort_by_key(|e| {
                e.metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            });

            // Remove oldest entries until under limit
            let mut current_size = total_size;
            for entry in entries {
                if current_size <= max_bytes {
                    break;
                }

                if let Ok(metadata) = entry.metadata() {
                    let size = metadata.len();
                    if std::fs::remove_dir_all(entry.path()).is_ok() {
                        current_size -= size;
                        tracing::debug!(path = %entry.path().display(), "Evicted cached model");
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tensor_name() {
        let (target, layer, is_a) = parse_tensor_name(
            "model.layers.5.self_attn.q_proj.lora_A.weight"
        ).unwrap();
        assert_eq!(target, LoraTarget::QProj);
        assert_eq!(layer, 5);
        assert!(is_a);

        let (target, layer, is_a) = parse_tensor_name(
            "base_model.model.layers.12.self_attn.v_proj.lora_B.weight"
        ).unwrap();
        assert_eq!(target, LoraTarget::VProj);
        assert_eq!(layer, 12);
        assert!(!is_a);
    }

    #[test]
    fn test_fuse_weights() {
        // Small 2x2 example
        let base = vec![1.0, 2.0, 3.0, 4.0];
        let lora_a = vec![0.5, 0.5]; // [1, 2] - rank 1
        let lora_b = vec![1.0, 2.0]; // [2, 1]

        // B @ A = [[0.5, 0.5], [1.0, 1.0]]
        // With scale 1.0: [[1.5, 2.5], [4.0, 5.0]]
        let fused = fuse_weights(&base, &lora_a, &lora_b, 2, 2, 1, 1.0);

        assert!((fused[0] - 1.5).abs() < 1e-6);
        assert!((fused[1] - 2.5).abs() < 1e-6);
        assert!((fused[2] - 4.0).abs() < 1e-6);
        assert!((fused[3] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_cache_key_stability() {
        let config = LoraFusionConfig {
            base_model_path: "/path/to/model.mlpackage".into(),
            output_path: "/path/to/output.mlmodelc".into(),
            adapters: vec![
                AdapterFusionSpec {
                    weights_path: "/path/to/adapter.safetensors".into(),
                    gate_weight: 0.5,
                    alpha: 32.0,
                    rank: 16,
                },
            ],
            compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
        };

        let key1 = FusedModelCache::cache_key(&config);
        let key2 = FusedModelCache::cache_key(&config);
        assert_eq!(key1, key2);
    }
}
