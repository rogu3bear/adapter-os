//! Offline LoRA Fusion for CoreML Backend
//!
//! This module implements **offline pre-fusion** of LoRA adapters into base model weights,
//! creating fused safetensors files that can be exported to CoreML `.mlpackage` format.
//!
//! ## Fusion Strategy
//!
//! CoreML models are compiled and opaque - they don't expose intermediate layer activations
//! at runtime. Therefore, this module provides **offline weight-space fusion** that:
//!
//! 1. Loads base model weights from safetensors format
//! 2. Loads LoRA adapter weights (A and B matrices) from safetensors
//! 3. Computes fused weights: W_fused = W_base + (alpha/rank) * sum(gate_i * B_i @ A_i)
//! 4. Writes fused weights to a new safetensors file
//! 5. The fused weights can then be converted to CoreML `.mlpackage` for ANE deployment
//!
//! ## NOT Runtime Fusion
//!
//! This module does **NOT** perform runtime LoRA fusion. For runtime adapter switching,
//! see the sidecar path (currently stubbed - requires Metal/MLX integration).
//!
//! Formula: W_fused = W_base + (alpha/rank) * sum(gate_i * B_i @ A_i)
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use adapteros_core::Result;
//! use adapteros_lora_kernel_coreml::fusion::{
//!     AdapterFusionSpec, LoraFusionConfig, fuse_lora_into_model,
//! };
//! use adapteros_lora_kernel_coreml::ComputeUnits;
//!
//! fn main() -> Result<()> {
//!     // Step 1: Pre-fuse LoRA weights into base model weights
//!     let config = LoraFusionConfig {
//!         base_model_path: "base_weights.safetensors".into(),  // Input: base weights
//!         output_path: "fused_weights.safetensors".into(),      // Output: fused weights
//!         adapters: vec![AdapterFusionSpec {
//!             weights_path: "adapter_a.safetensors".into(),
//!             gate_weight: 0.5,  // Q15 router weight
//!             alpha: 32.0,
//!             rank: 16,
//!         }],
//!         compute_units: ComputeUnits::CpuAndNeuralEngine,
//!     };
//!
//!     let result = fuse_lora_into_model(&config)?;
//!     println!("✅ Fused {} layers, {} params modified",
//!              result.layers_fused, result.total_params_modified);
//!
//!     // Step 2 (not shown): Convert fused_weights.safetensors to CoreML .mlpackage
//!     // using scripts/convert_mlx_to_coreml.py or coremltools
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Workflow Overview
//!
//! ```text
//! 1. Base Model (safetensors)
//!      ↓
//! 2. fuse_lora_into_model() ← Adapter weights (safetensors)
//!      ↓
//! 3. Fused Weights (safetensors)
//!      ↓
//! 4. Convert to CoreML (scripts/convert_mlx_to_coreml.py)
//!      ↓
//! 5. Fused Model (.mlpackage) → Deploy to ANE
//! ```

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Options for controlling fusion behavior
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FusionOptions {
    /// If true, shape mismatches cause hard errors instead of being skipped with warnings.
    /// Default is false for backwards compatibility.
    #[serde(default)]
    pub strict: bool,
}

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
    /// Fusion options
    #[serde(default)]
    pub options: FusionOptions,
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
            format!(
                "model.layers.{}.self_attn.{}.lora_A.weight",
                layer_idx, base
            ),
            format!(
                "model.layers.{}.self_attn.{}.lora_B.weight",
                layer_idx, base
            ),
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
    let layer_idx = name.split('.').find_map(|part| {
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

/// Validate LoRA matrix dimensions match expected shapes.
///
/// Returns structured error if dimensions don't match, enabling consistent
/// handling across batch and single-adapter fusion paths.
///
/// # Arguments
/// * `layer` - Layer index
/// * `target` - LoRA target module
/// * `lora_a` - LoRA A matrix
/// * `lora_b` - LoRA B matrix
/// * `rank` - Expected LoRA rank
/// * `in_dim` - Expected input dimension
/// * `out_dim` - Expected output dimension
///
/// # Returns
/// * `Ok(())` if dimensions match
/// * `Err(LoraShapeMismatch)` with structured error if dimensions don't match
pub fn validate_lora_shapes(
    layer: usize,
    target: &LoraTarget,
    lora_a: &[f32],
    lora_b: &[f32],
    rank: usize,
    in_dim: usize,
    out_dim: usize,
) -> Result<()> {
    let expected_a = rank * in_dim;
    let expected_b = out_dim * rank;

    if lora_a.len() != expected_a || lora_b.len() != expected_b {
        return Err(AosError::LoraShapeMismatch {
            layer,
            target: format!("{:?}", target),
            expected_a,
            got_a: lora_a.len(),
            expected_b,
            got_b: lora_b.len(),
        });
    }
    Ok(())
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

/// Fused weights for a single layer target
#[derive(Debug, Clone)]
pub struct FusedLayerWeights {
    /// Target module (q_proj, k_proj, etc.)
    pub target: LoraTarget,
    /// Layer index
    pub layer_idx: usize,
    /// Fused weight matrix [out_dim, in_dim]
    pub weights: Vec<f32>,
    /// Output dimension
    pub out_dim: usize,
    /// Input dimension
    pub in_dim: usize,
}

/// Complete fused model weights
#[derive(Debug)]
pub struct FusedModelWeights {
    /// All fused layer weights
    pub layers: Vec<FusedLayerWeights>,
    /// Total number of layers in the model
    pub num_layers: usize,
    /// Statistics about the fusion
    pub stats: FusionStats,
}

/// Statistics about a fusion operation
#[derive(Debug, Clone, Default)]
pub struct FusionStats {
    /// Number of layers where fusion was applied
    pub layers_fused: usize,
    /// Number of weight matrices fused per layer
    pub weights_per_layer: usize,
    /// Total number of parameters modified
    pub total_params_modified: usize,
    /// Adapters that were fused
    pub adapters_fused: usize,
}

/// Fuse LoRA adapters into base model weights (OFFLINE FUSION)
///
/// This performs **offline weight-space fusion** of LoRA adapter weights into
/// base model weights. The output is a safetensors file containing fused weights.
///
/// # Important: This is NOT Runtime Fusion
///
/// This function performs offline fusion and writes to disk. It does **NOT** modify
/// a loaded CoreML model at runtime. For runtime adapter switching, see the sidecar
/// path (currently stubbed).
///
/// # Process
/// 1. Load base model weights from safetensors
/// 2. Load LoRA adapter weights from safetensors
/// 3. For each target layer, compute: W_fused = W_base + sum(gate_i * alpha_i/rank_i * B_i @ A_i)
/// 4. Write fused weights to output safetensors file
/// 5. Caller can convert fused weights to CoreML `.mlpackage` using coremltools
///
/// # Formula
/// ```text
/// W_fused = W_base + sum_i(gate_i * alpha_i / rank_i * B_i @ A_i)
/// ```
///
/// Where:
/// - `W_base`: Original base model weights [out_dim, in_dim]
/// - `gate_i`: Q15 gate weight from router (0.0 to 1.0)
/// - `alpha_i`: LoRA alpha scaling factor
/// - `rank_i`: LoRA rank (dimension of low-rank decomposition)
/// - `A_i`: LoRA down-projection [rank, in_dim]
/// - `B_i`: LoRA up-projection [out_dim, rank]
///
/// # Use Cases
///
/// - **Production deployment**: Pre-fuse known adapter combinations for zero runtime overhead
/// - **Model distribution**: Ship fused models to clients (no adapter hot-swapping needed)
/// - **Audit trails**: Generate deterministic hashes of fused weight combinations
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_lora_kernel_coreml::fusion::{
///     LoraFusionConfig, AdapterFusionSpec, fuse_lora_into_model
/// };
/// use adapteros_lora_kernel_coreml::ComputeUnits;
///
/// let config = LoraFusionConfig {
///     base_model_path: "base_weights.safetensors".into(),
///     output_path: "fused_weights.safetensors".into(),
///     adapters: vec![
///         AdapterFusionSpec {
///             weights_path: "adapter.safetensors".into(),
///             gate_weight: 1.0,
///             alpha: 32.0,
///             rank: 16,
///         },
///     ],
///     compute_units: ComputeUnits::CpuAndNeuralEngine,
/// };
///
/// let result = fuse_lora_into_model(&config)?;
/// println!("Fused {} layers", result.layers_fused);
///
/// // Next step (not shown): Convert fused_weights.safetensors to CoreML
/// // using coremltools or scripts/convert_mlx_to_coreml.py
/// # Ok::<(), adapteros_core::AosError>(())
/// ```
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
    let mut all_adapter_weights = Vec::new();
    for adapter in &config.adapters {
        if !adapter.weights_path.exists() {
            return Err(AosError::NotFound(format!(
                "Adapter weights not found: {}",
                adapter.weights_path.display()
            )));
        }

        let weights = load_lora_weights(&adapter.weights_path)?;
        all_adapter_weights.push((adapter.clone(), weights));
    }

    // Perform the fusion
    let fused = fuse_adapters_to_weights(
        &config.base_model_path,
        &all_adapter_weights,
        config.options.strict,
    )?;

    // Write fused weights to output path
    write_fused_weights(&config.output_path, &fused)?;

    tracing::info!(
        output = %config.output_path.display(),
        layers = fused.stats.layers_fused,
        params = fused.stats.total_params_modified,
        adapters = fused.stats.adapters_fused,
        "Successfully fused LoRA adapters"
    );

    Ok(FusionResult {
        output_path: config.output_path.clone(),
        layers_fused: fused.stats.layers_fused,
        weights_per_layer: fused.stats.weights_per_layer,
        total_params_modified: fused.stats.total_params_modified,
    })
}

/// Fuse multiple adapters into base model weights (in-memory)
///
/// This is the core fusion algorithm that computes:
/// W_fused = W_base + sum_i(gate_i * alpha_i / rank_i * B_i @ A_i)
///
/// # Arguments
/// * `base_weights_path` - Path to base model weights
/// * `adapters` - List of adapters to fuse
/// * `strict` - If true, shape mismatches cause hard errors; if false, skip with warnings
pub fn fuse_adapters_to_weights(
    base_weights_path: &PathBuf,
    adapters: &[(AdapterFusionSpec, ParsedLoraWeights)],
    strict: bool,
) -> Result<FusedModelWeights> {
    // Load base model weights
    let base_file_data = std::fs::read(base_weights_path)
        .map_err(|e| AosError::Io(format!("Failed to read base weights: {}", e)))?;

    let base_tensors = safetensors::SafeTensors::deserialize(&base_file_data)
        .map_err(|e| AosError::Kernel(format!("Failed to parse base safetensors: {}", e)))?;

    // Detect number of layers from base model
    let num_layers = detect_num_layers(&base_tensors)?;

    let mut fused_layers = Vec::new();
    let mut stats = FusionStats {
        adapters_fused: adapters.len(),
        ..Default::default()
    };

    // Iterate through each layer and target
    for layer_idx in 0..num_layers {
        let mut layer_fused = false;

        for target in LoraTarget::all() {
            // Find base weight tensor for this target
            let base_key = get_base_weight_key(layer_idx, *target);

            let base_tensor = match base_tensors.tensor(&base_key) {
                Ok(t) => t,
                Err(_) => {
                    // Try alternative key patterns
                    match find_base_tensor_alternative(&base_tensors, layer_idx, *target) {
                        Some(t) => t,
                        None => continue, // Skip if not found
                    }
                }
            };

            // Parse base tensor shape and data
            let shape = base_tensor.shape();
            if shape.len() != 2 {
                tracing::warn!(
                    layer = layer_idx,
                    target = ?target,
                    shape = ?shape,
                    "Skipping non-2D weight tensor"
                );
                continue;
            }

            let out_dim = shape[0];
            let in_dim = shape[1];

            // Convert base tensor data to f32
            let mut fused_weights = tensor_to_f32_vec(base_tensor)?;

            // Apply each adapter's contribution
            for (adapter_spec, lora_weights) in adapters {
                // Get LoRA A and B matrices for this layer/target
                let a_matrices = match lora_weights.a_matrices.get(target) {
                    Some(m) => m,
                    None => continue,
                };
                let b_matrices = match lora_weights.b_matrices.get(target) {
                    Some(m) => m,
                    None => continue,
                };

                let lora_a = match a_matrices.get(&layer_idx) {
                    Some(a) => a,
                    None => continue,
                };
                let lora_b = match b_matrices.get(&layer_idx) {
                    Some(b) => b,
                    None => continue,
                };

                // Compute scale factor: gate * alpha / rank
                let rank = adapter_spec.rank;
                if rank == 0 {
                    return Err(AosError::Validation("LoRA rank cannot be zero".to_string()));
                }

                let scale = adapter_spec.gate_weight * adapter_spec.alpha / (rank as f32);

                // Validate dimensions using unified validation helper
                if let Err(e) =
                    validate_lora_shapes(layer_idx, target, lora_a, lora_b, rank, in_dim, out_dim)
                {
                    if strict {
                        // In strict mode, shape mismatches are fatal
                        return Err(e);
                    } else {
                        // In lenient mode, log warning and skip
                        tracing::warn!(
                            layer = layer_idx,
                            target = ?target,
                            error = %e,
                            "Shape mismatch, skipping adapter for this layer/target"
                        );
                        continue;
                    }
                }

                // Compute B @ A and add to fused weights
                // This is the core LoRA fusion: W_fused += scale * (B @ A)
                fuse_weights_inplace(
                    &mut fused_weights,
                    lora_a,
                    lora_b,
                    out_dim,
                    in_dim,
                    rank,
                    scale,
                );

                layer_fused = true;
            }

            // Store fused layer
            fused_layers.push(FusedLayerWeights {
                target: *target,
                layer_idx,
                weights: fused_weights,
                out_dim,
                in_dim,
            });

            stats.total_params_modified += out_dim * in_dim;
        }

        if layer_fused {
            stats.layers_fused += 1;
        }
    }

    // Calculate weights per layer
    if stats.layers_fused > 0 {
        stats.weights_per_layer = fused_layers.len() / stats.layers_fused;
    }

    Ok(FusedModelWeights {
        layers: fused_layers,
        num_layers,
        stats,
    })
}

/// Fuse weights in-place: fused += scale * (B @ A)
///
/// This modifies `fused` directly, adding the scaled LoRA contribution.
///
/// # Arguments
/// * `fused` - Mutable base weights to modify [out_dim * in_dim]
/// * `lora_a` - LoRA A matrix [rank, in_dim] (row-major)
/// * `lora_b` - LoRA B matrix [out_dim, rank] (row-major)
/// * `out_dim` - Output dimension
/// * `in_dim` - Input dimension
/// * `rank` - LoRA rank
/// * `scale` - Combined scale factor (gate * alpha / rank)
fn fuse_weights_inplace(
    fused: &mut [f32],
    lora_a: &[f32],
    lora_b: &[f32],
    out_dim: usize,
    in_dim: usize,
    rank: usize,
    scale: f32,
) {
    // Compute B @ A and add to fused weights
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
}

/// Detect number of layers in the model from tensor names
fn detect_num_layers(tensors: &safetensors::SafeTensors) -> Result<usize> {
    let mut max_layer = 0;
    let mut found_any = false;

    for name in tensors.names() {
        // Look for patterns like "model.layers.N." or "layers.N."
        for part in name.split('.') {
            if let Ok(n) = part.parse::<usize>() {
                max_layer = max_layer.max(n);
                found_any = true;
            }
        }
    }

    if !found_any {
        return Err(AosError::Kernel(
            "Could not detect number of layers in base model".to_string(),
        ));
    }

    Ok(max_layer + 1)
}

/// Get the expected key for base model weights
fn get_base_weight_key(layer_idx: usize, target: LoraTarget) -> String {
    let target_name = match target {
        LoraTarget::QProj => "q_proj",
        LoraTarget::KProj => "k_proj",
        LoraTarget::VProj => "v_proj",
        LoraTarget::OProj => "o_proj",
        LoraTarget::GateProj => "gate_proj",
        LoraTarget::UpProj => "up_proj",
        LoraTarget::DownProj => "down_proj",
    };

    // Common Llama/Mistral format
    format!(
        "model.layers.{}.self_attn.{}.weight",
        layer_idx, target_name
    )
}

/// Try alternative key patterns for finding base tensors
fn find_base_tensor_alternative<'a>(
    tensors: &'a safetensors::SafeTensors<'a>,
    layer_idx: usize,
    target: LoraTarget,
) -> Option<safetensors::tensor::TensorView<'a>> {
    let target_name = match target {
        LoraTarget::QProj => "q_proj",
        LoraTarget::KProj => "k_proj",
        LoraTarget::VProj => "v_proj",
        LoraTarget::OProj => "o_proj",
        LoraTarget::GateProj => "gate_proj",
        LoraTarget::UpProj => "up_proj",
        LoraTarget::DownProj => "down_proj",
    };

    // Alternative patterns used by different model formats
    let patterns = [
        // MLP projections
        format!("model.layers.{}.mlp.{}.weight", layer_idx, target_name),
        // Without model prefix
        format!("layers.{}.self_attn.{}.weight", layer_idx, target_name),
        format!("layers.{}.mlp.{}.weight", layer_idx, target_name),
        // Transformer prefix
        format!("transformer.h.{}.attn.{}.weight", layer_idx, target_name),
        format!("transformer.h.{}.mlp.{}.weight", layer_idx, target_name),
        // GPT-NeoX style
        format!(
            "gpt_neox.layers.{}.attention.{}.weight",
            layer_idx, target_name
        ),
    ];

    for pattern in &patterns {
        if let Ok(tensor) = tensors.tensor(pattern) {
            return Some(tensor);
        }
    }

    None
}

/// Convert a safetensors TensorView to Vec<f32>
fn tensor_to_f32_vec(tensor: safetensors::tensor::TensorView) -> Result<Vec<f32>> {
    let dtype = tensor.dtype();
    let data = tensor.data();

    match dtype {
        safetensors::Dtype::F32 => Ok(data
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()),
        safetensors::Dtype::F16 => {
            // Convert f16 to f32
            Ok(data
                .chunks_exact(2)
                .map(|b| {
                    let bits = u16::from_le_bytes([b[0], b[1]]);
                    half::f16::from_bits(bits).to_f32()
                })
                .collect())
        }
        safetensors::Dtype::BF16 => {
            // Convert bf16 to f32
            Ok(data
                .chunks_exact(2)
                .map(|b| {
                    let bits = u16::from_le_bytes([b[0], b[1]]);
                    half::bf16::from_bits(bits).to_f32()
                })
                .collect())
        }
        other => Err(AosError::Kernel(format!(
            "Unsupported tensor dtype: {:?}",
            other
        ))),
    }
}

/// Write fused weights to a safetensors file
fn write_fused_weights(output_path: &PathBuf, fused: &FusedModelWeights) -> Result<()> {
    // Create output directory if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;
    }

    // Build tensor data - collect byte representations
    let byte_tensors: Vec<(String, Vec<u8>, Vec<usize>)> = fused
        .layers
        .iter()
        .map(|layer| {
            let key = get_base_weight_key(layer.layer_idx, layer.target);
            let shape = vec![layer.out_dim, layer.in_dim];
            let bytes: Vec<u8> = layer.weights.iter().flat_map(|f| f.to_le_bytes()).collect();
            (key, bytes, shape)
        })
        .collect();

    // Create tensor views that reference the byte data
    let tensor_views: Vec<(&str, safetensors::tensor::TensorView)> = byte_tensors
        .iter()
        .filter_map(|(name, bytes, shape)| {
            safetensors::tensor::TensorView::new(safetensors::Dtype::F32, shape.clone(), bytes)
                .ok()
                .map(|tv| (name.as_str(), tv))
        })
        .collect();

    // Serialize using safetensors
    let serialized = safetensors::serialize(tensor_views.into_iter(), &None)
        .map_err(|e| AosError::Kernel(format!("Failed to serialize fused weights: {}", e)))?;

    std::fs::write(output_path, serialized)
        .map_err(|e| AosError::Io(format!("Failed to write fused weights: {}", e)))?;

    Ok(())
}

/// Fuse LoRA weights directly into base weights in memory (no file I/O)
///
/// This is a convenience function for fusing a single adapter into base weights
/// without writing to disk.
///
/// # Arguments
/// * `base_weights` - Mutable base weights to modify in-place
/// * `adapter` - Adapter specification with gate weight, alpha, and rank
/// * `lora_weights` - Parsed LoRA weights
/// * `target` - Target module (q_proj, k_proj, etc.)
/// * `layer_idx` - Layer index
/// * `out_dim` - Output dimension
/// * `in_dim` - Input dimension
///
/// # Returns
/// `true` if fusion was applied, `false` if LoRA weights were not found for this target/layer
pub fn fuse_single_adapter_inplace(
    base_weights: &mut [f32],
    adapter: &AdapterFusionSpec,
    lora_weights: &ParsedLoraWeights,
    target: LoraTarget,
    layer_idx: usize,
    out_dim: usize,
    in_dim: usize,
) -> Result<bool> {
    // Get LoRA A and B matrices for this layer/target
    let a_matrices = match lora_weights.a_matrices.get(&target) {
        Some(m) => m,
        None => return Ok(false),
    };
    let b_matrices = match lora_weights.b_matrices.get(&target) {
        Some(m) => m,
        None => return Ok(false),
    };

    let lora_a = match a_matrices.get(&layer_idx) {
        Some(a) => a,
        None => return Ok(false),
    };
    let lora_b = match b_matrices.get(&layer_idx) {
        Some(b) => b,
        None => return Ok(false),
    };

    // Compute scale factor: gate * alpha / rank
    let rank = adapter.rank;
    if rank == 0 {
        return Err(AosError::Validation("LoRA rank cannot be zero".to_string()));
    }

    let scale = adapter.gate_weight * adapter.alpha / (rank as f32);

    // Validate dimensions
    let expected_a_len = rank * in_dim;
    let expected_b_len = out_dim * rank;

    if lora_a.len() != expected_a_len || lora_b.len() != expected_b_len {
        return Err(AosError::Kernel(format!(
            "LoRA dimension mismatch: A expected {}, got {}; B expected {}, got {}",
            expected_a_len,
            lora_a.len(),
            expected_b_len,
            lora_b.len()
        )));
    }

    // Apply fusion
    fuse_weights_inplace(base_weights, lora_a, lora_b, out_dim, in_dim, rank, scale);

    Ok(true)
}

/// Batch fuse multiple adapters into base weights
///
/// This is optimized for fusing multiple adapters at once, computing the
/// combined delta matrix once rather than applying each adapter sequentially.
///
/// # Arguments
/// * `base_weights` - Base weights [out_dim, in_dim]
/// * `adapters` - List of (adapter_spec, lora_weights) pairs
/// * `target` - Target module
/// * `layer_idx` - Layer index
/// * `out_dim` - Output dimension
/// * `in_dim` - Input dimension
///
/// # Returns
/// Fused weights as a new vector
pub fn fuse_multiple_adapters(
    base_weights: &[f32],
    adapters: &[(AdapterFusionSpec, ParsedLoraWeights)],
    target: LoraTarget,
    layer_idx: usize,
    out_dim: usize,
    in_dim: usize,
) -> Result<Vec<f32>> {
    let mut fused = base_weights.to_vec();

    for (adapter_spec, lora_weights) in adapters {
        // Get LoRA A and B matrices
        let a_matrices = match lora_weights.a_matrices.get(&target) {
            Some(m) => m,
            None => continue,
        };
        let b_matrices = match lora_weights.b_matrices.get(&target) {
            Some(m) => m,
            None => continue,
        };

        let lora_a = match a_matrices.get(&layer_idx) {
            Some(a) => a,
            None => continue,
        };
        let lora_b = match b_matrices.get(&layer_idx) {
            Some(b) => b,
            None => continue,
        };

        let rank = adapter_spec.rank;
        if rank == 0 {
            return Err(AosError::Validation("LoRA rank cannot be zero".to_string()));
        }

        let scale = adapter_spec.gate_weight * adapter_spec.alpha / (rank as f32);

        // Validate dimensions
        let expected_a_len = rank * in_dim;
        let expected_b_len = out_dim * rank;

        if lora_a.len() != expected_a_len || lora_b.len() != expected_b_len {
            tracing::warn!(
                layer = layer_idx,
                target = ?target,
                "Skipping adapter due to dimension mismatch"
            );
            continue;
        }

        fuse_weights_inplace(&mut fused, lora_a, lora_b, out_dim, in_dim, rank, scale);
    }

    Ok(fused)
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
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            tracing::warn!(
                cache_dir = %cache_dir.display(),
                error = %e,
                "Failed to create fused model cache directory"
            );
        }
        Self {
            cache_dir,
            max_cache_size_gb,
        }
    }

    /// Get cache key for fusion configuration
    ///
    /// Uses BLAKE3 for deterministic hashing across process restarts.
    /// DefaultHasher is seeded with ASLR-derived values, producing
    /// different hashes on different runs.
    pub fn cache_key(config: &LoraFusionConfig) -> String {
        let mut hasher = blake3::Hasher::new();

        // Hash base model path
        hasher.update(config.base_model_path.to_string_lossy().as_bytes());

        // Hash each adapter configuration deterministically
        for adapter in &config.adapters {
            hasher.update(adapter.weights_path.to_string_lossy().as_bytes());
            hasher.update(&adapter.gate_weight.to_le_bytes());
            hasher.update(&adapter.alpha.to_le_bytes());
            hasher.update(&(adapter.rank as u32).to_le_bytes());
        }

        // Return first 16 hex chars for a compact but unique key
        hasher.finalize().to_hex()[..16].to_string()
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
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "mlmodelc"))
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
        let (target, layer, is_a) =
            parse_tensor_name("model.layers.5.self_attn.q_proj.lora_A.weight").unwrap();
        assert_eq!(target, LoraTarget::QProj);
        assert_eq!(layer, 5);
        assert!(is_a);

        let (target, layer, is_a) =
            parse_tensor_name("base_model.model.layers.12.self_attn.v_proj.lora_B.weight").unwrap();
        assert_eq!(target, LoraTarget::VProj);
        assert_eq!(layer, 12);
        assert!(!is_a);
    }

    #[test]
    fn test_parse_tensor_name_all_targets() {
        // Test all target types
        let targets = [
            ("q_proj", LoraTarget::QProj),
            ("k_proj", LoraTarget::KProj),
            ("v_proj", LoraTarget::VProj),
            ("o_proj", LoraTarget::OProj),
            ("gate_proj", LoraTarget::GateProj),
            ("up_proj", LoraTarget::UpProj),
            ("down_proj", LoraTarget::DownProj),
        ];

        for (name, expected_target) in targets {
            let tensor_name = format!("model.layers.0.self_attn.{}.lora_A.weight", name);
            let (target, layer, is_a) = parse_tensor_name(&tensor_name).unwrap();
            assert_eq!(target, expected_target);
            assert_eq!(layer, 0);
            assert!(is_a);
        }
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
    fn test_fuse_weights_inplace() {
        // Test in-place fusion
        let mut base = vec![1.0, 2.0, 3.0, 4.0];
        let lora_a = vec![0.5, 0.5]; // [1, 2] - rank 1
        let lora_b = vec![1.0, 2.0]; // [2, 1]

        fuse_weights_inplace(&mut base, &lora_a, &lora_b, 2, 2, 1, 1.0);

        assert!((base[0] - 1.5).abs() < 1e-6);
        assert!((base[1] - 2.5).abs() < 1e-6);
        assert!((base[2] - 4.0).abs() < 1e-6);
        assert!((base[3] - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_fuse_weights_with_scaling() {
        // Test with alpha scaling (typical LoRA usage: alpha=32, rank=16 -> scale=2)
        let base = vec![1.0, 2.0, 3.0, 4.0];
        let lora_a = vec![0.5, 0.5]; // [1, 2] - rank 1
        let lora_b = vec![1.0, 2.0]; // [2, 1]

        // B @ A = [[0.5, 0.5], [1.0, 1.0]]
        // With scale 2.0: base + 2 * [[0.5, 0.5], [1.0, 1.0]] = [[2.0, 3.0], [5.0, 6.0]]
        let fused = fuse_weights(&base, &lora_a, &lora_b, 2, 2, 1, 2.0);

        assert!((fused[0] - 2.0).abs() < 1e-6);
        assert!((fused[1] - 3.0).abs() < 1e-6);
        assert!((fused[2] - 5.0).abs() < 1e-6);
        assert!((fused[3] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_fuse_weights_rank2() {
        // Test with rank 2
        // base: 2x3 matrix
        let base = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        // A: [2, 3] - rank 2, in_dim 3
        let lora_a = vec![
            1.0, 0.0, 0.0, // rank 0
            0.0, 1.0, 0.0, // rank 1
        ];
        // B: [2, 2] - out_dim 2, rank 2
        let lora_b = vec![
            1.0, 2.0, // out 0
            3.0, 4.0, // out 1
        ];

        // B @ A:
        // [1*1 + 2*0, 1*0 + 2*1, 1*0 + 2*0] = [1, 2, 0]
        // [3*1 + 4*0, 3*0 + 4*1, 3*0 + 4*0] = [3, 4, 0]
        // Result with scale 1.0:
        // base + [[1, 2, 0], [3, 4, 0]] = [[2, 4, 3], [7, 9, 6]]
        let fused = fuse_weights(&base, &lora_a, &lora_b, 2, 3, 2, 1.0);

        assert!((fused[0] - 2.0).abs() < 1e-6);
        assert!((fused[1] - 4.0).abs() < 1e-6);
        assert!((fused[2] - 3.0).abs() < 1e-6);
        assert!((fused[3] - 7.0).abs() < 1e-6);
        assert!((fused[4] - 9.0).abs() < 1e-6);
        assert!((fused[5] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_fuse_weights_gate_blending() {
        // Test Q15 gate weight blending (simulating router output)
        let base = vec![1.0, 2.0, 3.0, 4.0];
        let lora_a = vec![1.0, 1.0]; // [1, 2] - rank 1
        let lora_b = vec![1.0, 1.0]; // [2, 1]

        // B @ A = [[1, 1], [1, 1]]
        // With gate=0.5, alpha=2, rank=1 -> scale = 0.5 * 2 / 1 = 1.0
        // Result: base + 1.0 * [[1, 1], [1, 1]] = [[2, 3], [4, 5]]
        let scale = 0.5 * 2.0 / 1.0;
        let fused = fuse_weights(&base, &lora_a, &lora_b, 2, 2, 1, scale);

        assert!((fused[0] - 2.0).abs() < 1e-6);
        assert!((fused[1] - 3.0).abs() < 1e-6);
        assert!((fused[2] - 4.0).abs() < 1e-6);
        assert!((fused[3] - 5.0).abs() < 1e-6);

        // Test with lower gate (0.25)
        let scale_low = 0.25 * 2.0 / 1.0;
        let fused_low = fuse_weights(&base, &lora_a, &lora_b, 2, 2, 1, scale_low);

        assert!((fused_low[0] - 1.5).abs() < 1e-6);
        assert!((fused_low[1] - 2.5).abs() < 1e-6);
        assert!((fused_low[2] - 3.5).abs() < 1e-6);
        assert!((fused_low[3] - 4.5).abs() < 1e-6);
    }

    #[test]
    fn test_lora_target_all() {
        let targets = LoraTarget::all();
        assert_eq!(targets.len(), 7);
        assert!(targets.contains(&LoraTarget::QProj));
        assert!(targets.contains(&LoraTarget::KProj));
        assert!(targets.contains(&LoraTarget::VProj));
        assert!(targets.contains(&LoraTarget::OProj));
        assert!(targets.contains(&LoraTarget::GateProj));
        assert!(targets.contains(&LoraTarget::UpProj));
        assert!(targets.contains(&LoraTarget::DownProj));
    }

    #[test]
    fn test_lora_target_to_pattern() {
        let (a_key, b_key) = LoraTarget::QProj.to_safetensor_pattern(5);
        assert_eq!(a_key, "model.layers.5.self_attn.q_proj.lora_A.weight");
        assert_eq!(b_key, "model.layers.5.self_attn.q_proj.lora_B.weight");

        let (a_key, b_key) = LoraTarget::DownProj.to_safetensor_pattern(10);
        assert_eq!(a_key, "model.layers.10.self_attn.down_proj.lora_A.weight");
        assert_eq!(b_key, "model.layers.10.self_attn.down_proj.lora_B.weight");
    }

    #[test]
    fn test_get_base_weight_key() {
        assert_eq!(
            get_base_weight_key(0, LoraTarget::QProj),
            "model.layers.0.self_attn.q_proj.weight"
        );
        assert_eq!(
            get_base_weight_key(5, LoraTarget::VProj),
            "model.layers.5.self_attn.v_proj.weight"
        );
        assert_eq!(
            get_base_weight_key(31, LoraTarget::DownProj),
            "model.layers.31.self_attn.down_proj.weight"
        );
    }

    #[test]
    fn test_fusion_stats_default() {
        let stats = FusionStats::default();
        assert_eq!(stats.layers_fused, 0);
        assert_eq!(stats.weights_per_layer, 0);
        assert_eq!(stats.total_params_modified, 0);
        assert_eq!(stats.adapters_fused, 0);
    }

    #[test]
    fn test_cache_key_stability() {
        let config = LoraFusionConfig {
            base_model_path: "/path/to/model.mlpackage".into(),
            output_path: "/path/to/output.mlmodelc".into(),
            adapters: vec![AdapterFusionSpec {
                weights_path: "/path/to/adapter.safetensors".into(),
                gate_weight: 0.5,
                alpha: 32.0,
                rank: 16,
            }],
            compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
            options: FusionOptions::default(),
        };

        let key1 = FusedModelCache::cache_key(&config);
        let key2 = FusedModelCache::cache_key(&config);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_changes_with_config() {
        let config1 = LoraFusionConfig {
            base_model_path: "/path/to/model.mlpackage".into(),
            output_path: "/path/to/output.mlmodelc".into(),
            adapters: vec![AdapterFusionSpec {
                weights_path: "/path/to/adapter.safetensors".into(),
                gate_weight: 0.5,
                alpha: 32.0,
                rank: 16,
            }],
            compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
            options: FusionOptions::default(),
        };

        let config2 = LoraFusionConfig {
            base_model_path: "/path/to/model.mlpackage".into(),
            output_path: "/path/to/output.mlmodelc".into(),
            adapters: vec![AdapterFusionSpec {
                weights_path: "/path/to/adapter.safetensors".into(),
                gate_weight: 0.7, // Different gate weight
                alpha: 32.0,
                rank: 16,
            }],
            compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
            options: FusionOptions::default(),
        };

        let key1 = FusedModelCache::cache_key(&config1);
        let key2 = FusedModelCache::cache_key(&config2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_validation_empty_adapters() {
        // Create a temporary file for base model path (so we don't get NotFound error)
        let temp_root = std::path::PathBuf::from("var/tmp");
        std::fs::create_dir_all(&temp_root).unwrap();
        let temp_dir = tempfile::TempDir::new_in(&temp_root).unwrap();
        let temp_base = temp_dir.path().join("test_base_model.safetensors");

        // Create empty file (doesn't need to be valid safetensors for this validation test)
        std::fs::write(&temp_base, b"").unwrap();

        let config = LoraFusionConfig {
            base_model_path: temp_base.clone(),
            output_path: temp_dir.path().join("test_output.safetensors"),
            adapters: vec![],
            compute_units: crate::ComputeUnits::CpuAndNeuralEngine,
            options: FusionOptions::default(),
        };

        let result = fuse_lora_into_model(&config);

        assert!(result.is_err());
        match result {
            Err(AosError::Validation(msg)) => {
                assert!(msg.contains("adapter"));
            }
            _ => panic!("Expected Validation error, got {:?}", result),
        }
    }

    #[test]
    fn test_multiple_adapters_accumulate() {
        // Test that multiple adapters contribute additively
        let base = vec![1.0, 2.0, 3.0, 4.0];
        let lora_a = vec![1.0, 1.0]; // [1, 2] - rank 1
        let lora_b = vec![1.0, 1.0]; // [2, 1]

        // First adapter: scale 1.0
        let mut fused = base.clone();
        fuse_weights_inplace(&mut fused, &lora_a, &lora_b, 2, 2, 1, 1.0);

        // Second adapter: scale 1.0 (same contribution)
        fuse_weights_inplace(&mut fused, &lora_a, &lora_b, 2, 2, 1, 1.0);

        // Should be base + 2 * [[1, 1], [1, 1]] = [[3, 4], [5, 6]]
        assert!((fused[0] - 3.0).abs() < 1e-6);
        assert!((fused[1] - 4.0).abs() < 1e-6);
        assert!((fused[2] - 5.0).abs() < 1e-6);
        assert!((fused[3] - 6.0).abs() < 1e-6);
    }
}
