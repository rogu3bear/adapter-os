//! ANE Optimization Techniques and Guidelines
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This module documents and implements ANE optimization strategies for maximum
//! performance and power efficiency.

use adapteros_core::{AosError, Result};
use tracing::{debug, info, warn};

/// ANE optimization guidelines
///
/// These rules ensure optimal execution on Apple Neural Engine:
///
/// ## Memory Layout Optimizations
///
/// 1. **NCHW Format**: ANE prefers channel-first layout (Num, Channel, Height, Width)
///    - Input tensors should be in NCHW format
///    - Weight tensors should be packed in ANE-friendly layout
///    - Avoid NHWC format which requires expensive transposes
///
/// 2. **Tensor Alignment**: All tensor dimensions should be multiples of 16
///    - ANE processes data in 16-element vector units
///    - Padding to align dimensions improves throughput
///    - Example: 3584 → 3584 (already aligned), 3580 → 3584 (pad 4)
///
/// 3. **Float16 Precision**: Use 16-bit floating point for ANE
///    - ANE is optimized for Float16 operations
///    - 2x memory bandwidth vs Float32
///    - Minimal accuracy loss for inference
///
/// ## Kernel Fusion Techniques
///
/// 1. **Fuse Sequential Operations**: Combine ops to reduce memory transfers
///    - MatMul + Bias → Single fused op
///    - MatMul + Activation → Single fused op
///    - Gate application + Accumulation → Single kernel
///
/// 2. **Minimize CPU ↔ ANE Transfers**: Keep data on ANE as long as possible
///    - Batch multiple operations before synchronizing
///    - Use Metal shared memory for CPU/ANE coordination
///    - Avoid frequent readbacks to CPU
///
/// 3. **Parallel Module Execution**: Execute independent modules concurrently
///    - Each adapter module can run in parallel
///    - Use Metal command buffer parallelism
///    - ANE has 16 cores for concurrent execution
///
/// ## Optimal Tensor Shapes
///
/// For LoRA operations on ANE:
///
/// - **Down-projection**: (H, R) where H % 16 == 0 and R % 16 == 0
///   - Recommended: H=3584, R=16 (both aligned)
///
/// - **Up-projection**: (R, H) where R % 16 == 0 and H % 16 == 0
///   - Recommended: R=16, H=3584 (both aligned)
///
/// - **Batch size**: Prefer powers of 2 (1, 2, 4, 8)
///   - ANE can process multiple batches in parallel
///   - Larger batches amortize kernel launch overhead
///
/// ## Power Efficiency Tips
///
/// 1. **Prefer ANE over GPU**: ANE is ~3-5x more power efficient
///    - ANE: ~15 TOPS/W
///    - GPU: ~3-5 TOPS/W
///
/// 2. **Batch Operations**: Group multiple inference requests
///    - Reduces per-request overhead
///    - Better ANE utilization
///
/// 3. **Quantization**: Use INT8 or INT4 when possible
///    - Further power savings
///    - Requires calibration for accuracy
///
/// ## Common Pitfalls
///
/// ❌ **DON'T**:
/// - Use Float32 precision (ANE prefers Float16)
/// - Create frequent CPU ↔ ANE synchronization points
/// - Use unaligned tensor dimensions
/// - Mix ANE and GPU operations without batching
///
/// ✅ **DO**:
/// - Align all dimensions to multiples of 16
/// - Use Float16 precision
/// - Batch operations to minimize transfers
/// - Profile ANE utilization to verify optimization

/// Memory layout optimizer for ANE
pub struct ANEMemoryLayoutOptimizer {
    /// Alignment requirement (16 for ANE)
    alignment: usize,
}

impl ANEMemoryLayoutOptimizer {
    /// Create new memory layout optimizer
    pub fn new() -> Self {
        Self { alignment: 16 }
    }

    /// Pad dimension to next aligned value
    ///
    /// # Arguments
    /// * `dim` - Original dimension size
    ///
    /// # Returns
    /// Padded dimension (multiple of alignment)
    pub fn pad_dimension(&self, dim: usize) -> usize {
        let remainder = dim % self.alignment;
        if remainder == 0 {
            dim
        } else {
            dim + (self.alignment - remainder)
        }
    }

    /// Check if tensor shape is ANE-optimal
    ///
    /// # Arguments
    /// * `shape` - Tensor shape (e.g., [batch, seq_len, hidden])
    ///
    /// # Returns
    /// true if all dimensions are aligned
    pub fn is_optimal_shape(&self, shape: &[usize]) -> bool {
        shape.iter().all(|&dim| dim % self.alignment == 0)
    }

    /// Optimize tensor shape for ANE
    ///
    /// # Arguments
    /// * `shape` - Original tensor shape
    ///
    /// # Returns
    /// Optimized shape with padding
    pub fn optimize_shape(&self, shape: &[usize]) -> Vec<usize> {
        shape.iter().map(|&dim| self.pad_dimension(dim)).collect()
    }

    /// Calculate padding required for each dimension
    ///
    /// # Arguments
    /// * `shape` - Original tensor shape
    ///
    /// # Returns
    /// Padding required for each dimension
    pub fn calculate_padding(&self, shape: &[usize]) -> Vec<usize> {
        shape
            .iter()
            .map(|&dim| {
                let padded = self.pad_dimension(dim);
                padded - dim
            })
            .collect()
    }

    /// Validate shape is suitable for ANE
    pub fn validate_shape(&self, shape: &[usize]) -> Result<()> {
        if shape.is_empty() {
            return Err(AosError::Config("Empty tensor shape".to_string()));
        }

        let unaligned: Vec<usize> = shape
            .iter()
            .enumerate()
            .filter_map(|(idx, &dim)| {
                if dim % self.alignment != 0 {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        if !unaligned.is_empty() {
            warn!(
                "Tensor shape {:?} has unaligned dimensions at indices {:?} (alignment={})",
                shape, unaligned, self.alignment
            );
            debug!("Consider padding to: {:?}", self.optimize_shape(shape));
        }

        Ok(())
    }
}

impl Default for ANEMemoryLayoutOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// ANE kernel fusion planner
///
/// Analyzes computation graph to identify fusion opportunities
pub struct ANEKernelFusionPlanner {
    /// Enabled fusion strategies
    enabled_fusions: Vec<FusionStrategy>,
}

/// Fusion strategies for ANE
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionStrategy {
    /// Fuse MatMul + Bias
    MatMulBias,
    /// Fuse MatMul + Activation
    MatMulActivation,
    /// Fuse Gate Application + Accumulation
    GatedAccumulation,
    /// Fuse Down-Projection + Up-Projection
    LoRAProjection,
}

impl ANEKernelFusionPlanner {
    /// Create new fusion planner with default strategies
    pub fn new() -> Self {
        Self {
            enabled_fusions: vec![
                FusionStrategy::MatMulBias,
                FusionStrategy::MatMulActivation,
                FusionStrategy::GatedAccumulation,
                FusionStrategy::LoRAProjection,
            ],
        }
    }

    /// Check if fusion strategy is enabled
    pub fn is_enabled(&self, strategy: FusionStrategy) -> bool {
        self.enabled_fusions.contains(&strategy)
    }

    /// Enable fusion strategy
    pub fn enable(&mut self, strategy: FusionStrategy) {
        if !self.enabled_fusions.contains(&strategy) {
            self.enabled_fusions.push(strategy);
            info!("Enabled fusion strategy: {:?}", strategy);
        }
    }

    /// Disable fusion strategy
    pub fn disable(&mut self, strategy: FusionStrategy) {
        self.enabled_fusions.retain(|&s| s != strategy);
        info!("Disabled fusion strategy: {:?}", strategy);
    }

    /// Get recommended fusion for LoRA operations
    pub fn recommend_lora_fusion(&self) -> Vec<FusionStrategy> {
        vec![
            FusionStrategy::GatedAccumulation,
            FusionStrategy::LoRAProjection,
        ]
    }
}

impl Default for ANEKernelFusionPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// ANE performance tuning parameters
#[derive(Debug, Clone)]
pub struct ANETuningParams {
    /// Tile size for matrix multiplication (must be multiple of 16)
    pub tile_size: usize,
    /// Batch size for inference
    pub batch_size: usize,
    /// Enable kernel fusion
    pub enable_fusion: bool,
    /// Use Float16 precision
    pub use_float16: bool,
    /// Maximum concurrent modules
    pub max_concurrent_modules: usize,
    /// Enable aggressive memory optimization
    pub aggressive_memory_opt: bool,
}

impl Default for ANETuningParams {
    fn default() -> Self {
        Self {
            tile_size: 16,
            batch_size: 1,
            enable_fusion: true,
            use_float16: true,
            max_concurrent_modules: 8,
            aggressive_memory_opt: false,
        }
    }
}

impl ANETuningParams {
    /// Create tuning parameters optimized for latency
    pub fn optimize_for_latency() -> Self {
        Self {
            tile_size: 16,
            batch_size: 1,
            enable_fusion: true,
            use_float16: true,
            max_concurrent_modules: 4,
            aggressive_memory_opt: false,
        }
    }

    /// Create tuning parameters optimized for throughput
    pub fn optimize_for_throughput() -> Self {
        Self {
            tile_size: 32,
            batch_size: 8,
            enable_fusion: true,
            use_float16: true,
            max_concurrent_modules: 16,
            aggressive_memory_opt: true,
        }
    }

    /// Create tuning parameters optimized for power efficiency
    pub fn optimize_for_power() -> Self {
        Self {
            tile_size: 16,
            batch_size: 4,
            enable_fusion: true,
            use_float16: true,
            max_concurrent_modules: 8,
            aggressive_memory_opt: true,
        }
    }

    /// Validate tuning parameters
    pub fn validate(&self) -> Result<()> {
        if self.tile_size % 16 != 0 {
            return Err(AosError::Config(format!(
                "tile_size must be multiple of 16, got {}",
                self.tile_size
            )));
        }

        if self.batch_size == 0 {
            return Err(AosError::Config("batch_size must be > 0".to_string()));
        }

        if self.max_concurrent_modules == 0 {
            return Err(AosError::Config(
                "max_concurrent_modules must be > 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// ANE activation function optimization
pub mod ane_activations {
    use super::*;

    /// ANE-friendly activation functions
    ///
    /// ANE has hardware support for:
    /// - ReLU (very efficient)
    /// - Sigmoid (efficient)
    /// - Tanh (efficient)
    /// - GELU (moderate efficiency)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ANEActivation {
        /// Rectified Linear Unit (fastest on ANE)
        ReLU,
        /// Sigmoid activation
        Sigmoid,
        /// Hyperbolic tangent
        Tanh,
        /// Gaussian Error Linear Unit
        GELU,
    }

    impl ANEActivation {
        /// Get relative performance on ANE (1.0 = fastest)
        pub fn ane_performance_factor(&self) -> f32 {
            match self {
                ANEActivation::ReLU => 1.0,
                ANEActivation::Sigmoid => 0.9,
                ANEActivation::Tanh => 0.9,
                ANEActivation::GELU => 0.7,
            }
        }

        /// Check if activation is ANE-native
        pub fn is_ane_native(&self) -> bool {
            matches!(
                self,
                ANEActivation::ReLU | ANEActivation::Sigmoid | ANEActivation::Tanh
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_layout_optimizer() {
        let optimizer = ANEMemoryLayoutOptimizer::new();

        assert_eq!(optimizer.pad_dimension(3584), 3584); // Already aligned
        assert_eq!(optimizer.pad_dimension(3580), 3584); // Pad 4
        assert_eq!(optimizer.pad_dimension(15), 16); // Pad 1

        let shape = vec![1, 1024, 3584];
        assert!(optimizer.is_optimal_shape(&shape));

        let unaligned_shape = vec![1, 1023, 3584];
        assert!(!optimizer.is_optimal_shape(&unaligned_shape));

        let optimized = optimizer.optimize_shape(&unaligned_shape);
        assert_eq!(optimized, vec![16, 1024, 3584]);
    }

    #[test]
    fn test_fusion_planner() {
        let mut planner = ANEKernelFusionPlanner::new();

        assert!(planner.is_enabled(FusionStrategy::MatMulBias));
        assert!(planner.is_enabled(FusionStrategy::GatedAccumulation));

        planner.disable(FusionStrategy::MatMulBias);
        assert!(!planner.is_enabled(FusionStrategy::MatMulBias));

        planner.enable(FusionStrategy::MatMulBias);
        assert!(planner.is_enabled(FusionStrategy::MatMulBias));
    }

    #[test]
    fn test_tuning_params_validation() {
        let valid_params = ANETuningParams::default();
        assert!(valid_params.validate().is_ok());

        let invalid_tile = ANETuningParams {
            tile_size: 15, // Not multiple of 16
            ..Default::default()
        };
        assert!(invalid_tile.validate().is_err());

        let invalid_batch = ANETuningParams {
            batch_size: 0,
            ..Default::default()
        };
        assert!(invalid_batch.validate().is_err());
    }

    #[test]
    fn test_tuning_presets() {
        let latency = ANETuningParams::optimize_for_latency();
        assert_eq!(latency.batch_size, 1);
        assert_eq!(latency.tile_size, 16);

        let throughput = ANETuningParams::optimize_for_throughput();
        assert_eq!(throughput.batch_size, 8);
        assert_eq!(throughput.tile_size, 32);

        let power = ANETuningParams::optimize_for_power();
        assert!(power.aggressive_memory_opt);
        assert_eq!(power.batch_size, 4);
    }

    #[test]
    fn test_ane_activation_performance() {
        use ane_activations::ANEActivation;

        assert_eq!(ANEActivation::ReLU.ane_performance_factor(), 1.0);
        assert!(ANEActivation::GELU.ane_performance_factor() < 1.0);

        assert!(ANEActivation::ReLU.is_ane_native());
        assert!(ANEActivation::Sigmoid.is_ane_native());
    }
}
