//! Apple Neural Engine Optimizer
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! Provides ANE-specific optimizations including Float16 precision,
//! memory alignment, weight packing, and adaptive optimization strategies.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// ANE optimizer for model and execution optimization
#[derive(Debug)]
pub struct ANEOptimizer {
    /// Optimization configuration
    config: OptimizerConfig,
    /// Cached tensor alignments
    tensor_alignments: HashMap<String, TensorAlignment>,
    /// Operation compatibility cache
    op_compatibility: HashMap<String, ANECompatibility>,
}

/// Optimizer configuration
#[derive(Debug, Clone)]
pub struct OptimizerConfig {
    /// Use Float16 precision throughout
    pub use_float16: bool,
    /// Align tensor dimensions to multiples of 16
    pub align_dimensions: bool,
    /// Pack weights for ANE format
    pub pack_weights: bool,
    /// Enable adaptive precision selection
    pub adaptive_precision: bool,
    /// Enable thermal-aware scheduling
    pub thermal_aware: bool,
    /// Enable battery-aware operation
    pub battery_aware: bool,
    /// Target memory bandwidth (GB/s)
    pub target_bandwidth_gbps: f32,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            use_float16: true,
            align_dimensions: true,
            pack_weights: true,
            adaptive_precision: true,
            thermal_aware: true,
            battery_aware: true,
            target_bandwidth_gbps: 100.0,
        }
    }
}

/// Tensor alignment information
#[derive(Debug, Clone)]
pub struct TensorAlignment {
    /// Original shape
    pub original_shape: Vec<usize>,
    /// Aligned shape (multiples of 16)
    pub aligned_shape: Vec<usize>,
    /// Padding required per dimension
    pub padding: Vec<usize>,
    /// Total memory overhead (bytes)
    pub memory_overhead: usize,
}

/// ANE compatibility status for operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ANECompatibility {
    /// Fully compatible with ANE
    FullyCompatible,
    /// Compatible with modifications
    CompatibleWithModifications(Vec<String>),
    /// Requires fallback to GPU/CPU
    RequiresFallback(String),
}

/// Precision mode for execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecisionMode {
    /// Float32 (highest accuracy, slowest)
    Float32,
    /// Float16 (balanced, ANE-optimized)
    Float16,
    /// Int8 (quantized, fastest, lower accuracy)
    Int8,
    /// Mixed precision (adaptive)
    Mixed,
}

/// Operation descriptor for compatibility checking
#[derive(Debug, Clone)]
pub struct OperationDescriptor {
    /// Operation type
    pub op_type: String,
    /// Input tensor shapes
    pub input_shapes: Vec<Vec<usize>>,
    /// Output tensor shapes
    pub output_shapes: Vec<Vec<usize>>,
    /// Data types
    pub data_types: Vec<DataType>,
    /// Attributes
    pub attributes: HashMap<String, String>,
}

/// Data type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Float32,
    Float16,
    Int8,
    Int32,
    Bool,
}

/// Weight packing format for ANE
#[derive(Debug, Clone)]
pub struct PackedWeights {
    /// Packed data buffer
    pub data: Vec<u8>,
    /// Original shape
    pub original_shape: Vec<usize>,
    /// Packed shape
    pub packed_shape: Vec<usize>,
    /// Data type
    pub dtype: DataType,
    /// Packing format metadata
    pub format_metadata: HashMap<String, String>,
}

/// Adaptive optimization strategy
#[derive(Debug, Clone)]
pub struct AdaptiveStrategy {
    /// Current precision mode
    pub precision_mode: PrecisionMode,
    /// Current thermal state
    pub thermal_state: ThermalState,
    /// Current power mode
    pub power_mode: PowerMode,
    /// Optimization recommendations
    pub recommendations: Vec<String>,
}

/// Thermal state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// Power mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    /// Maximum performance
    Performance,
    /// Balanced mode
    Balanced,
    /// Battery saver mode
    LowPower,
}

impl ANEOptimizer {
    /// Create a new ANE optimizer
    pub fn new(config: OptimizerConfig) -> Self {
        info!("ANE optimizer initialized with config: {:?}", config);

        Self {
            config,
            tensor_alignments: HashMap::new(),
            op_compatibility: HashMap::new(),
        }
    }

    /// Align tensor dimensions to multiples of 16 for ANE
    pub fn align_tensor_shape(&mut self, tensor_id: String, shape: Vec<usize>) -> Result<TensorAlignment> {
        if !self.config.align_dimensions {
            return Ok(TensorAlignment {
                original_shape: shape.clone(),
                aligned_shape: shape.clone(),
                padding: vec![0; shape.len()],
                memory_overhead: 0,
            });
        }

        let mut aligned_shape = Vec::new();
        let mut padding = Vec::new();
        let mut total_overhead = 0;

        for &dim in &shape {
            let aligned_dim = if dim % 16 == 0 {
                dim
            } else {
                ((dim + 15) / 16) * 16
            };

            let pad = aligned_dim - dim;
            aligned_shape.push(aligned_dim);
            padding.push(pad);
            total_overhead += pad;
        }

        let alignment = TensorAlignment {
            original_shape: shape,
            aligned_shape,
            padding,
            memory_overhead: total_overhead * std::mem::size_of::<f16>(),
        };

        debug!(
            "Aligned tensor {}: {:?} -> {:?} (overhead: {} bytes)",
            tensor_id, alignment.original_shape, alignment.aligned_shape, alignment.memory_overhead
        );

        self.tensor_alignments.insert(tensor_id, alignment.clone());
        Ok(alignment)
    }

    /// Check operation compatibility with ANE
    pub fn check_operation_compatibility(&mut self, op: &OperationDescriptor) -> Result<ANECompatibility> {
        // Check cache first
        let cache_key = format!("{}_{:?}", op.op_type, op.input_shapes);
        if let Some(cached) = self.op_compatibility.get(&cache_key) {
            return Ok(cached.clone());
        }

        let compatibility = match op.op_type.as_str() {
            // Fully compatible operations
            "MatMul" | "Conv2D" | "LayerNorm" | "GELU" | "Softmax" | "Reshape" => {
                self.check_matmul_compatibility(op)?
            }

            // Operations requiring modifications
            "BatchNorm" => ANECompatibility::CompatibleWithModifications(vec![
                "Convert to LayerNorm for better ANE support".to_string(),
            ]),

            "Attention" => {
                if self.check_attention_compatibility(op) {
                    ANECompatibility::FullyCompatible
                } else {
                    ANECompatibility::CompatibleWithModifications(vec![
                        "Split into ANE-compatible MatMul operations".to_string(),
                        "Use Flash Attention approximation".to_string(),
                    ])
                }
            }

            // Operations requiring fallback
            "Custom" | "Loop" | "If" => ANECompatibility::RequiresFallback(
                "Custom operations not supported on ANE".to_string(),
            ),

            _ => {
                warn!("Unknown operation type for ANE compatibility: {}", op.op_type);
                ANECompatibility::RequiresFallback(format!(
                    "Unknown operation type: {}",
                    op.op_type
                ))
            }
        };

        self.op_compatibility.insert(cache_key, compatibility.clone());
        Ok(compatibility)
    }

    /// Check MatMul compatibility
    fn check_matmul_compatibility(&self, op: &OperationDescriptor) -> Result<ANECompatibility> {
        if op.input_shapes.is_empty() {
            return Ok(ANECompatibility::RequiresFallback(
                "No input shapes provided".to_string(),
            ));
        }

        let mut modifications = Vec::new();

        // Check matrix dimensions
        for (i, shape) in op.input_shapes.iter().enumerate() {
            if shape.len() < 2 {
                return Ok(ANECompatibility::RequiresFallback(
                    "MatMul requires 2D or higher tensors".to_string(),
                ));
            }

            let last_dim = shape[shape.len() - 1];
            if last_dim % 8 != 0 {
                modifications.push(format!(
                    "Input {} last dimension ({}) should be multiple of 8 for optimal ANE performance",
                    i, last_dim
                ));
            }
        }

        // Check data types
        for (i, dtype) in op.data_types.iter().enumerate() {
            if *dtype != DataType::Float16 && *dtype != DataType::Float32 {
                modifications.push(format!(
                    "Input {} data type {:?} not optimal for ANE, consider Float16",
                    i, dtype
                ));
            }
        }

        if modifications.is_empty() {
            Ok(ANECompatibility::FullyCompatible)
        } else {
            Ok(ANECompatibility::CompatibleWithModifications(modifications))
        }
    }

    /// Check attention compatibility
    fn check_attention_compatibility(&self, op: &OperationDescriptor) -> bool {
        // Check if attention operation meets ANE requirements
        if let Some(seq_len_str) = op.attributes.get("sequence_length") {
            if let Ok(seq_len) = seq_len_str.parse::<usize>() {
                // ANE prefers sequence lengths that are multiples of 16
                return seq_len % 16 == 0 && seq_len <= 2048;
            }
        }
        false
    }

    /// Pack weights for ANE format
    pub fn pack_weights(&self, weights: &[f32], shape: Vec<usize>) -> Result<PackedWeights> {
        if !self.config.pack_weights {
            // No packing, just convert to bytes
            let bytes: Vec<u8> = weights
                .iter()
                .flat_map(|&f| f.to_le_bytes())
                .collect();

            return Ok(PackedWeights {
                data: bytes,
                original_shape: shape.clone(),
                packed_shape: shape,
                dtype: DataType::Float32,
                format_metadata: HashMap::new(),
            });
        }

        // Convert to Float16 for ANE
        let f16_data = if self.config.use_float16 {
            self.convert_to_float16(weights)?
        } else {
            weights
                .iter()
                .flat_map(|&f| f.to_le_bytes())
                .collect()
        };

        let mut metadata = HashMap::new();
        metadata.insert("format".to_string(), "ANE_packed".to_string());
        metadata.insert("alignment".to_string(), "16".to_string());

        Ok(PackedWeights {
            data: f16_data,
            original_shape: shape.clone(),
            packed_shape: shape,
            dtype: if self.config.use_float16 {
                DataType::Float16
            } else {
                DataType::Float32
            },
            format_metadata: metadata,
        })
    }

    /// Convert Float32 to Float16
    fn convert_to_float16(&self, data: &[f32]) -> Result<Vec<u8>> {
        let mut f16_bytes = Vec::with_capacity(data.len() * 2);

        for &f in data {
            let f16 = half::f16::from_f32(f);
            f16_bytes.extend_from_slice(&f16.to_le_bytes());
        }

        debug!("Converted {} Float32 values to Float16", data.len());
        Ok(f16_bytes)
    }

    /// Determine adaptive optimization strategy
    pub fn determine_adaptive_strategy(
        &self,
        thermal_state: ThermalState,
        battery_level: Option<f32>,
        accuracy_requirement: f32,
    ) -> Result<AdaptiveStrategy> {
        let mut recommendations = Vec::new();

        // Determine power mode
        let power_mode = if let Some(battery) = battery_level {
            if self.config.battery_aware {
                if battery < 0.2 {
                    recommendations.push("Battery low, using power-saving mode".to_string());
                    PowerMode::LowPower
                } else if battery < 0.5 {
                    PowerMode::Balanced
                } else {
                    PowerMode::Performance
                }
            } else {
                PowerMode::Performance
            }
        } else {
            PowerMode::Performance
        };

        // Determine precision mode based on thermal state and requirements
        let precision_mode = if !self.config.adaptive_precision {
            if self.config.use_float16 {
                PrecisionMode::Float16
            } else {
                PrecisionMode::Float32
            }
        } else {
            match thermal_state {
                ThermalState::Critical => {
                    recommendations.push("Critical thermal state, reducing to Int8".to_string());
                    PrecisionMode::Int8
                }
                ThermalState::Serious => {
                    if accuracy_requirement > 0.95 {
                        recommendations.push(
                            "High thermal but accuracy critical, using Float16".to_string(),
                        );
                        PrecisionMode::Float16
                    } else {
                        recommendations.push("High thermal, using Int8 for efficiency".to_string());
                        PrecisionMode::Int8
                    }
                }
                ThermalState::Fair => {
                    if power_mode == PowerMode::LowPower {
                        PrecisionMode::Int8
                    } else {
                        PrecisionMode::Float16
                    }
                }
                ThermalState::Nominal => {
                    if accuracy_requirement > 0.98 {
                        PrecisionMode::Float32
                    } else {
                        PrecisionMode::Float16
                    }
                }
            }
        };

        // Add thermal-specific recommendations
        if self.config.thermal_aware {
            match thermal_state {
                ThermalState::Serious | ThermalState::Critical => {
                    recommendations.push("Consider reducing batch size or sequence length".to_string());
                    recommendations.push("Enable inference throttling".to_string());
                }
                _ => {}
            }
        }

        info!(
            "Adaptive strategy: {:?} precision, {:?} power, thermal: {:?}",
            precision_mode, power_mode, thermal_state
        );

        Ok(AdaptiveStrategy {
            precision_mode,
            thermal_state,
            power_mode,
            recommendations,
        })
    }

    /// Optimize tensor shape for ANE memory access
    pub fn optimize_memory_layout(&self, shape: &[usize], dtype: DataType) -> Result<Vec<usize>> {
        let element_size = match dtype {
            DataType::Float32 => 4,
            DataType::Float16 => 2,
            DataType::Int8 => 1,
            DataType::Int32 => 4,
            DataType::Bool => 1,
        };

        // ANE prefers memory-aligned shapes
        let mut optimized = shape.to_vec();

        // Align last dimension to cache line (64 bytes)
        if let Some(last) = optimized.last_mut() {
            let bytes_per_row = *last * element_size;
            if bytes_per_row % 64 != 0 {
                let aligned_elements = ((bytes_per_row + 63) / 64) * 64 / element_size;
                *last = aligned_elements;
            }
        }

        debug!(
            "Optimized memory layout: {:?} -> {:?} (dtype: {:?})",
            shape, optimized, dtype
        );

        Ok(optimized)
    }

    /// Generate ANE compatibility report
    pub fn generate_compatibility_report(&self) -> Vec<(String, ANECompatibility)> {
        self.op_compatibility.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

// Float16 shim for when half crate is not available
#[cfg(not(feature = "half"))]
mod half {
    pub struct f16(u16);

    impl f16 {
        pub fn from_f32(f: f32) -> Self {
            // Simple IEEE 754 conversion (simplified)
            let bits = f.to_bits();
            let sign = (bits >> 31) & 1;
            let exp = ((bits >> 23) & 0xFF) as i32;
            let mant = bits & 0x7FFFFF;

            let f16_exp = if exp == 0 {
                0
            } else if exp == 0xFF {
                0x1F
            } else {
                let e = exp - 127 + 15;
                e.max(0).min(31) as u16
            };

            let f16_mant = (mant >> 13) as u16;
            let f16_bits = ((sign as u16) << 15) | (f16_exp << 10) | f16_mant;

            Self(f16_bits)
        }

        pub fn to_le_bytes(self) -> [u8; 2] {
            self.0.to_le_bytes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_alignment() {
        let mut optimizer = ANEOptimizer::new(OptimizerConfig::default());

        let shape = vec![13, 27, 35];
        let alignment = optimizer.align_tensor_shape("test_tensor".to_string(), shape).unwrap();

        assert_eq!(alignment.aligned_shape, vec![16, 32, 48]);
        assert_eq!(alignment.padding, vec![3, 5, 13]);
    }

    #[test]
    fn test_matmul_compatibility() {
        let optimizer = ANEOptimizer::new(OptimizerConfig::default());

        let op = OperationDescriptor {
            op_type: "MatMul".to_string(),
            input_shapes: vec![vec![1, 128, 768], vec![768, 768]],
            output_shapes: vec![vec![1, 128, 768]],
            data_types: vec![DataType::Float16, DataType::Float16],
            attributes: HashMap::new(),
        };

        let compat = optimizer.check_matmul_compatibility(&op).unwrap();
        assert_eq!(compat, ANECompatibility::FullyCompatible);
    }

    #[test]
    fn test_adaptive_strategy() {
        let optimizer = ANEOptimizer::new(OptimizerConfig::default());

        let strategy = optimizer.determine_adaptive_strategy(
            ThermalState::Nominal,
            Some(0.8),
            0.99,
        ).unwrap();

        assert!(matches!(strategy.precision_mode, PrecisionMode::Float16));
        assert_eq!(strategy.power_mode, PowerMode::Performance);
    }

    #[test]
    fn test_weight_packing() {
        let optimizer = ANEOptimizer::new(OptimizerConfig::default());

        let weights = vec![1.0, 2.0, 3.0, 4.0];
        let shape = vec![2, 2];

        let packed = optimizer.pack_weights(&weights, shape).unwrap();
        assert_eq!(packed.dtype, DataType::Float16);
        assert_eq!(packed.data.len(), weights.len() * 2); // 2 bytes per Float16
    }
}
