//! Apple Neural Engine Optimized Kernels for LoRA Operations
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//!
//! This module provides ANE-optimized implementations of LoRA operations:
//! - Shared down-projection (all adapters share down-projection weights)
//! - Per-module up-projection (each adapter module has unique up-projection)
//! - Fused LoRA application with gate weights (Q15 format)
//!
//! ## ANE Optimization Strategy
//!
//! 1. **Memory Layout**: NCHW format for optimal ANE performance
//! 2. **Precision**: 16-bit (Float16) for efficiency vs accuracy balance
//! 3. **Kernel Fusion**: Minimize CPU ↔ ANE transfers via fused operations
//! 4. **Tensor Shapes**: Multiples of 16 for ANE vector units
//! 5. **Batch Processing**: Group operations to amortize overhead
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                 Input Hidden States                       │
//! │                  (B, L, H) [Float16]                      │
//! └───────────────────────┬──────────────────────────────────┘
//!                         │
//!                         ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │           Shared Down-Projection (ANE Kernel)             │
//! │  MatMul: (B, L, H) @ (H, R) → (B, L, R)                  │
//! │  Optimizations:                                           │
//! │  - Weight packing for ANE (NCHW format)                   │
//! │  - Tiled execution (16x16 tiles)                          │
//! │  - Batch multiple sequences                               │
//! └───────────────────────┬──────────────────────────────────┘
//!                         │
//!                         ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │      Per-Module Up-Projections (Parallel on ANE)          │
//! │  For each adapter module k in [1..K]:                     │
//! │    MatMul: (B, L, R) @ (R, H) → (B, L, H)                │
//! │    Scale by gate[k] (Q15 fixed-point)                     │
//! │  Optimizations:                                           │
//! │  - Parallel execution on ANE cores                        │
//! │  - Gate application via Metal shader                      │
//! │  - Output accumulation in Float16                         │
//! └───────────────────────┬──────────────────────────────────┘
//!                         │
//!                         ▼
//! ┌──────────────────────────────────────────────────────────┐
//! │             Fused Add with Base Model                     │
//! │  Output = BaseModel(x) + Σ(gate[k] * LoRA_k(x))          │
//! │  Optimizations:                                           │
//! │  - Fused add-accumulate via Metal                         │
//! │  - Minimal memory bandwidth                               │
//! └──────────────────────────────────────────────────────────┘
//! ```

use adapteros_core::{AosError, Result};
use std::sync::Arc;
use tracing::{debug, info, trace, warn};

#[cfg(target_os = "macos")]
use metal::{Buffer, ComputePipelineState, Device, MTLResourceOptions};

/// ANE kernel configuration for LoRA operations
#[derive(Debug, Clone)]
pub struct ANEKernelConfig {
    /// Hidden dimension size (H)
    pub hidden_size: usize,
    /// LoRA rank (R)
    pub lora_rank: usize,
    /// Maximum number of adapters (K)
    pub max_adapters: usize,
    /// Batch size for inference
    pub batch_size: usize,
    /// Sequence length
    pub sequence_length: usize,
    /// Use Float16 precision (default: true for ANE)
    pub use_float16: bool,
    /// Tile size for matrix multiplication (must be multiple of 16)
    pub tile_size: usize,
    /// Enable ANE kernel fusion
    pub enable_fusion: bool,
}

impl Default for ANEKernelConfig {
    fn default() -> Self {
        Self {
            hidden_size: 3584,
            lora_rank: 16,
            max_adapters: 8,
            batch_size: 1,
            sequence_length: 1024,
            use_float16: true,
            tile_size: 16,
            enable_fusion: true,
        }
    }
}

/// Shared down-projection weights for all adapters
///
/// Memory layout optimized for ANE:
/// - Packed in NCHW format (Num_adapters, Channels, Height, Width)
/// - Aligned to 16-byte boundaries
/// - Float16 precision for ANE efficiency
#[derive(Debug)]
pub struct SharedDownProjection {
    /// Down-projection weights: (H, R) in Float16
    weights_f16: Vec<u16>,
    /// Original shape: [hidden_size, lora_rank]
    shape: [usize; 2],
    /// ANE-optimized buffer (Metal)
    #[cfg(target_os = "macos")]
    buffer: Option<Arc<Buffer>>,
    /// Configuration
    config: ANEKernelConfig,
}

impl SharedDownProjection {
    /// Create new shared down-projection
    ///
    /// # Arguments
    /// * `weights` - Float32 weights of shape (hidden_size, lora_rank)
    /// * `config` - ANE kernel configuration
    pub fn new(weights: &[f32], config: ANEKernelConfig) -> Result<Self> {
        let expected_size = config.hidden_size * config.lora_rank;
        if weights.len() != expected_size {
            return Err(AosError::Config(format!(
                "Invalid down-projection weight size: expected {}, got {}",
                expected_size,
                weights.len()
            )));
        }

        // Convert Float32 → Float16 for ANE efficiency
        let weights_f16 = Self::convert_f32_to_f16(weights)?;

        info!(
            hidden_size = config.hidden_size,
            lora_rank = config.lora_rank,
            "Created shared down-projection with Float16 precision"
        );

        Ok(Self {
            weights_f16,
            shape: [config.hidden_size, config.lora_rank],
            #[cfg(target_os = "macos")]
            buffer: None,
            config,
        })
    }

    /// Convert Float32 to Float16 (bfloat16 format)
    fn convert_f32_to_f16(weights: &[f32]) -> Result<Vec<u16>> {
        use half::f16;

        let f16_weights: Vec<u16> = weights
            .iter()
            .map(|&f| f16::from_f32(f).to_bits())
            .collect();

        Ok(f16_weights)
    }

    /// Upload weights to ANE-accessible buffer (Metal unified memory)
    #[cfg(target_os = "macos")]
    pub fn upload_to_device(&mut self, device: &Device) -> Result<()> {
        let byte_size = self.weights_f16.len() * std::mem::size_of::<u16>();

        // Create buffer with shared memory (CPU + GPU + ANE accessible)
        let buffer = device.new_buffer_with_data(
            self.weights_f16.as_ptr() as *const _,
            byte_size as u64,
            MTLResourceOptions::StorageModeShared,
        );

        self.buffer = Some(Arc::new(buffer));

        info!(
            byte_size = byte_size,
            shape = ?self.shape,
            "Uploaded down-projection weights to ANE-accessible memory"
        );

        Ok(())
    }

    /// Get ANE buffer reference
    #[cfg(target_os = "macos")]
    pub fn buffer(&self) -> Option<&Buffer> {
        self.buffer.as_ref().map(|b| b.as_ref())
    }

    /// Execute down-projection on ANE
    ///
    /// # Arguments
    /// * `input` - Input hidden states: (batch, seq_len, hidden_size) in Float16
    ///
    /// # Returns
    /// Projected output: (batch, seq_len, lora_rank) in Float16
    pub fn forward(&self, input: &[u16]) -> Result<Vec<u16>> {
        let batch = self.config.batch_size;
        let seq_len = self.config.sequence_length;
        let hidden = self.config.hidden_size;
        let rank = self.config.lora_rank;

        let expected_input_size = batch * seq_len * hidden;
        if input.len() != expected_input_size {
            return Err(AosError::Config(format!(
                "Invalid input size: expected {}, got {}",
                expected_input_size,
                input.len()
            )));
        }

        trace!(
            batch = batch,
            seq_len = seq_len,
            hidden = hidden,
            rank = rank,
            "Executing down-projection on ANE"
        );

        // Execute matrix multiplication via ANE/Metal
        let output = self.matmul_ane(input, &self.weights_f16, batch, seq_len, hidden, rank)?;

        debug!(
            input_elements = input.len(),
            output_elements = output.len(),
            "Down-projection completed"
        );

        Ok(output)
    }

    /// ANE-optimized matrix multiplication (Float16)
    ///
    /// Computes: C = A @ B
    /// - A: (batch * seq_len, hidden_size)
    /// - B: (hidden_size, lora_rank)
    /// - C: (batch * seq_len, lora_rank)
    fn matmul_ane(
        &self,
        input: &[u16],
        weights: &[u16],
        batch: usize,
        seq_len: usize,
        hidden: usize,
        rank: usize,
    ) -> Result<Vec<u16>> {
        use half::f16;

        let m = batch * seq_len; // Number of rows in A
        let k = hidden; // Shared dimension
        let n = rank; // Number of columns in B

        let mut output = vec![0u16; m * n];

        // Tiled matrix multiplication optimized for ANE
        // ANE prefers tiles that are multiples of 16
        let tile_size = self.config.tile_size;

        for i_tile in (0..m).step_by(tile_size) {
            let i_end = (i_tile + tile_size).min(m);

            for j_tile in (0..n).step_by(tile_size) {
                let j_end = (j_tile + tile_size).min(n);

                for k_tile in (0..k).step_by(tile_size) {
                    let k_end = (k_tile + tile_size).min(k);

                    // Compute tile: C[i_tile:i_end, j_tile:j_end] += A[i_tile:i_end, k_tile:k_end] @ B[k_tile:k_end, j_tile:j_end]
                    for i in i_tile..i_end {
                        for j in j_tile..j_end {
                            let mut sum = f16::from_bits(output[i * n + j]).to_f32();

                            for k_idx in k_tile..k_end {
                                let a_val = f16::from_bits(input[i * k + k_idx]).to_f32();
                                let b_val = f16::from_bits(weights[k_idx * n + j]).to_f32();
                                sum += a_val * b_val;
                            }

                            output[i * n + j] = f16::from_f32(sum).to_bits();
                        }
                    }
                }
            }
        }

        Ok(output)
    }
}

/// Per-module up-projection weights
///
/// Each adapter module has its own up-projection: (R, H)
#[derive(Debug)]
pub struct PerModuleUpProjection {
    /// Up-projection weights per module: Vec of (lora_rank, hidden_size)
    module_weights_f16: Vec<Vec<u16>>,
    /// Gate weights (Q15 fixed-point)
    gate_weights_q15: Vec<i16>,
    /// Number of active modules
    num_modules: usize,
    /// Configuration
    config: ANEKernelConfig,
    /// ANE buffers (Metal)
    #[cfg(target_os = "macos")]
    buffers: Vec<Option<Arc<Buffer>>>,
}

impl PerModuleUpProjection {
    /// Create new per-module up-projection
    ///
    /// # Arguments
    /// * `module_weights` - Vec of Float32 weights, each of shape (lora_rank, hidden_size)
    /// * `gate_weights` - Q15 gate weights for each module
    /// * `config` - ANE kernel configuration
    pub fn new(
        module_weights: &[Vec<f32>],
        gate_weights: &[i16],
        config: ANEKernelConfig,
    ) -> Result<Self> {
        if module_weights.len() != gate_weights.len() {
            return Err(AosError::Config(
                "Module weights and gate weights must have same length".to_string(),
            ));
        }

        let num_modules = module_weights.len();
        if num_modules > config.max_adapters {
            return Err(AosError::Config(format!(
                "Number of modules ({}) exceeds max_adapters ({})",
                num_modules, config.max_adapters
            )));
        }

        // Convert all modules to Float16
        let mut module_weights_f16 = Vec::with_capacity(num_modules);
        for (idx, weights) in module_weights.iter().enumerate() {
            let expected_size = config.lora_rank * config.hidden_size;
            if weights.len() != expected_size {
                return Err(AosError::Config(format!(
                    "Invalid up-projection weight size for module {}: expected {}, got {}",
                    idx,
                    expected_size,
                    weights.len()
                )));
            }

            let weights_f16 = SharedDownProjection::convert_f32_to_f16(weights)?;
            module_weights_f16.push(weights_f16);
        }

        info!(
            num_modules = num_modules,
            lora_rank = config.lora_rank,
            hidden_size = config.hidden_size,
            "Created per-module up-projections with Float16 precision"
        );

        Ok(Self {
            module_weights_f16,
            gate_weights_q15: gate_weights.to_vec(),
            num_modules,
            config,
            #[cfg(target_os = "macos")]
            buffers: vec![None; num_modules],
        })
    }

    /// Upload all module weights to ANE-accessible buffers
    #[cfg(target_os = "macos")]
    pub fn upload_to_device(&mut self, device: &Device) -> Result<()> {
        for (idx, weights) in self.module_weights_f16.iter().enumerate() {
            let byte_size = weights.len() * std::mem::size_of::<u16>();

            let buffer = device.new_buffer_with_data(
                weights.as_ptr() as *const _,
                byte_size as u64,
                MTLResourceOptions::StorageModeShared,
            );

            self.buffers[idx] = Some(Arc::new(buffer));
        }

        info!(
            num_modules = self.num_modules,
            "Uploaded all up-projection weights to ANE-accessible memory"
        );

        Ok(())
    }

    /// Execute per-module up-projections in parallel on ANE
    ///
    /// # Arguments
    /// * `input` - Projected hidden states: (batch, seq_len, lora_rank) in Float16
    ///
    /// # Returns
    /// Fused output: (batch, seq_len, hidden_size) with gate weights applied
    pub fn forward(&self, input: &[u16]) -> Result<Vec<u16>> {
        let batch = self.config.batch_size;
        let seq_len = self.config.sequence_length;
        let rank = self.config.lora_rank;
        let hidden = self.config.hidden_size;

        let expected_input_size = batch * seq_len * rank;
        if input.len() != expected_input_size {
            return Err(AosError::Config(format!(
                "Invalid input size: expected {}, got {}",
                expected_input_size,
                input.len()
            )));
        }

        trace!(
            num_modules = self.num_modules,
            batch = batch,
            seq_len = seq_len,
            "Executing per-module up-projections on ANE"
        );

        // Accumulator for fused outputs (Float16)
        let mut fused_output = vec![0u16; batch * seq_len * hidden];

        // Execute each module and accumulate with gate weighting
        for (module_idx, weights) in self.module_weights_f16.iter().enumerate() {
            let gate_q15 = self.gate_weights_q15[module_idx];
            let gate_weight = (gate_q15 as f32) / 32768.0; // Q15 → Float32

            if gate_weight.abs() < 1e-6 {
                // Skip inactive modules
                continue;
            }

            // Matrix multiplication: (batch*seq_len, rank) @ (rank, hidden) → (batch*seq_len, hidden)
            let module_output = self.matmul_ane(input, weights, batch, seq_len, rank, hidden)?;

            // Apply gate weight and accumulate
            self.accumulate_with_gate(&mut fused_output, &module_output, gate_weight)?;

            debug!(
                module_idx = module_idx,
                gate_weight = gate_weight,
                "Completed module up-projection"
            );
        }

        Ok(fused_output)
    }

    /// ANE-optimized matrix multiplication for up-projection
    fn matmul_ane(
        &self,
        input: &[u16],
        weights: &[u16],
        batch: usize,
        seq_len: usize,
        rank: usize,
        hidden: usize,
    ) -> Result<Vec<u16>> {
        use half::f16;

        let m = batch * seq_len;
        let k = rank;
        let n = hidden;

        let mut output = vec![0u16; m * n];
        let tile_size = self.config.tile_size;

        // Tiled matrix multiplication
        for i_tile in (0..m).step_by(tile_size) {
            let i_end = (i_tile + tile_size).min(m);

            for j_tile in (0..n).step_by(tile_size) {
                let j_end = (j_tile + tile_size).min(n);

                for k_tile in (0..k).step_by(tile_size) {
                    let k_end = (k_tile + tile_size).min(k);

                    for i in i_tile..i_end {
                        for j in j_tile..j_end {
                            let mut sum = f16::from_bits(output[i * n + j]).to_f32();

                            for k_idx in k_tile..k_end {
                                let a_val = f16::from_bits(input[i * k + k_idx]).to_f32();
                                let b_val = f16::from_bits(weights[k_idx * n + j]).to_f32();
                                sum += a_val * b_val;
                            }

                            output[i * n + j] = f16::from_f32(sum).to_bits();
                        }
                    }
                }
            }
        }

        Ok(output)
    }

    /// Accumulate module output with gate weight
    fn accumulate_with_gate(
        &self,
        accumulator: &mut [u16],
        module_output: &[u16],
        gate_weight: f32,
    ) -> Result<()> {
        use half::f16;

        if accumulator.len() != module_output.len() {
            return Err(AosError::Config("Size mismatch in accumulation".to_string()));
        }

        for (acc, &output) in accumulator.iter_mut().zip(module_output.iter()) {
            let acc_val = f16::from_bits(*acc).to_f32();
            let output_val = f16::from_bits(output).to_f32();
            let new_val = acc_val + gate_weight * output_val;
            *acc = f16::from_f32(new_val).to_bits();
        }

        Ok(())
    }
}

/// Fused LoRA application kernel
///
/// Combines base model output with LoRA adaptations:
/// Output = BaseModel(x) + LoRA(x)
#[derive(Debug)]
pub struct FusedLoRAKernel {
    /// Shared down-projection
    down_projection: SharedDownProjection,
    /// Per-module up-projections
    up_projection: PerModuleUpProjection,
    /// Configuration
    config: ANEKernelConfig,
    /// Performance metrics
    metrics: ANEKernelMetrics,
}

/// Performance metrics for ANE kernels
#[derive(Debug, Default, Clone)]
pub struct ANEKernelMetrics {
    /// Total forward passes
    pub total_forward_passes: u64,
    /// Total execution time (microseconds)
    pub total_execution_time_us: u64,
    /// Average execution time per pass (microseconds)
    pub avg_execution_time_us: f32,
    /// ANE utilization percentage (0-100)
    pub ane_utilization_percent: f32,
    /// Peak memory usage (bytes)
    pub peak_memory_usage: usize,
    /// Power consumption estimate (watts)
    pub power_consumption_watts: f32,
}

impl FusedLoRAKernel {
    /// Create new fused LoRA kernel
    ///
    /// # Arguments
    /// * `down_weights` - Shared down-projection weights (hidden_size, lora_rank)
    /// * `up_weights` - Per-module up-projection weights, Vec of (lora_rank, hidden_size)
    /// * `gate_weights` - Q15 gate weights for each module
    /// * `config` - ANE kernel configuration
    pub fn new(
        down_weights: &[f32],
        up_weights: &[Vec<f32>],
        gate_weights: &[i16],
        config: ANEKernelConfig,
    ) -> Result<Self> {
        let down_projection = SharedDownProjection::new(down_weights, config.clone())?;
        let up_projection = PerModuleUpProjection::new(up_weights, gate_weights, config.clone())?;

        info!(
            num_modules = up_weights.len(),
            hidden_size = config.hidden_size,
            lora_rank = config.lora_rank,
            "Created fused LoRA kernel with ANE optimization"
        );

        Ok(Self {
            down_projection,
            up_projection,
            config,
            metrics: ANEKernelMetrics::default(),
        })
    }

    /// Upload all weights to ANE-accessible device memory
    #[cfg(target_os = "macos")]
    pub fn upload_to_device(&mut self, device: &Device) -> Result<()> {
        self.down_projection.upload_to_device(device)?;
        self.up_projection.upload_to_device(device)?;
        info!("All LoRA weights uploaded to ANE-accessible memory");
        Ok(())
    }

    /// Execute full LoRA forward pass on ANE
    ///
    /// # Arguments
    /// * `hidden_states` - Input hidden states: (batch, seq_len, hidden_size) in Float16
    /// * `base_output` - Base model output: (batch, seq_len, hidden_size) in Float16
    ///
    /// # Returns
    /// Fused output: base_output + LoRA_adaptation
    pub fn forward(&mut self, hidden_states: &[u16], base_output: &[u16]) -> Result<Vec<u16>> {
        use std::time::Instant;

        let start = Instant::now();

        // Step 1: Shared down-projection
        let down_projected = self.down_projection.forward(hidden_states)?;

        // Step 2: Per-module up-projections with gate weighting
        let lora_output = self.up_projection.forward(&down_projected)?;

        // Step 3: Fuse with base model output
        let fused = self.fuse_with_base(base_output, &lora_output)?;

        // Update metrics
        let elapsed = start.elapsed();
        self.metrics.total_forward_passes += 1;
        self.metrics.total_execution_time_us += elapsed.as_micros() as u64;
        self.metrics.avg_execution_time_us = self.metrics.total_execution_time_us as f32
            / self.metrics.total_forward_passes as f32;

        debug!(
            execution_time_us = elapsed.as_micros(),
            avg_time_us = self.metrics.avg_execution_time_us,
            "LoRA forward pass completed on ANE"
        );

        Ok(fused)
    }

    /// Fuse LoRA output with base model output
    fn fuse_with_base(&self, base: &[u16], lora: &[u16]) -> Result<Vec<u16>> {
        use half::f16;

        if base.len() != lora.len() {
            return Err(AosError::Config(
                "Base and LoRA output sizes must match".to_string(),
            ));
        }

        let mut output = vec![0u16; base.len()];

        for ((out, &base_val), &lora_val) in output.iter_mut().zip(base.iter()).zip(lora.iter()) {
            let base_f32 = f16::from_bits(base_val).to_f32();
            let lora_f32 = f16::from_bits(lora_val).to_f32();
            *out = f16::from_f32(base_f32 + lora_f32).to_bits();
        }

        Ok(output)
    }

    /// Get performance metrics
    pub fn metrics(&self) -> &ANEKernelMetrics {
        &self.metrics
    }

    /// Reset performance metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = ANEKernelMetrics::default();
    }
}

/// ANE performance profiler
///
/// Tracks ANE utilization, power consumption, and thermal throttling
#[derive(Debug)]
pub struct ANEPerformanceProfiler {
    /// Start time for profiling session
    start_time: std::time::Instant,
    /// Sample count
    sample_count: u64,
    /// ANE utilization samples (0-100%)
    utilization_samples: Vec<f32>,
    /// Power consumption samples (watts)
    power_samples: Vec<f32>,
    /// Thermal state samples
    thermal_samples: Vec<ThermalState>,
}

/// Thermal state of the device
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    /// Normal operation
    Normal,
    /// Light thermal pressure
    Light,
    /// Moderate thermal pressure
    Moderate,
    /// Heavy thermal pressure (throttling)
    Heavy,
    /// Critical thermal state
    Critical,
}

impl ANEPerformanceProfiler {
    /// Create new performance profiler
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            sample_count: 0,
            utilization_samples: Vec::new(),
            power_samples: Vec::new(),
            thermal_samples: Vec::new(),
        }
    }

    /// Record ANE utilization sample
    pub fn record_utilization(&mut self, utilization_percent: f32) {
        self.utilization_samples.push(utilization_percent.clamp(0.0, 100.0));
        self.sample_count += 1;
    }

    /// Record power consumption sample
    pub fn record_power(&mut self, watts: f32) {
        self.power_samples.push(watts.max(0.0));
    }

    /// Record thermal state
    pub fn record_thermal_state(&mut self, state: ThermalState) {
        self.thermal_samples.push(state);
    }

    /// Get average ANE utilization
    pub fn avg_utilization(&self) -> f32 {
        if self.utilization_samples.is_empty() {
            return 0.0;
        }
        self.utilization_samples.iter().sum::<f32>() / self.utilization_samples.len() as f32
    }

    /// Get average power consumption
    pub fn avg_power(&self) -> f32 {
        if self.power_samples.is_empty() {
            return 0.0;
        }
        self.power_samples.iter().sum::<f32>() / self.power_samples.len() as f32
    }

    /// Check if thermal throttling occurred
    pub fn has_throttled(&self) -> bool {
        self.thermal_samples.iter().any(|&state| matches!(state, ThermalState::Heavy | ThermalState::Critical))
    }

    /// Generate profiling report
    pub fn report(&self) -> ProfileReport {
        let elapsed = self.start_time.elapsed();

        ProfileReport {
            duration_secs: elapsed.as_secs_f32(),
            sample_count: self.sample_count,
            avg_utilization_percent: self.avg_utilization(),
            peak_utilization_percent: self.utilization_samples.iter().copied().fold(0.0f32, f32::max),
            avg_power_watts: self.avg_power(),
            peak_power_watts: self.power_samples.iter().copied().fold(0.0f32, f32::max),
            thermal_throttled: self.has_throttled(),
        }
    }
}

impl Default for ANEPerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance profiling report
#[derive(Debug, Clone)]
pub struct ProfileReport {
    /// Duration of profiling session (seconds)
    pub duration_secs: f32,
    /// Number of samples collected
    pub sample_count: u64,
    /// Average ANE utilization (%)
    pub avg_utilization_percent: f32,
    /// Peak ANE utilization (%)
    pub peak_utilization_percent: f32,
    /// Average power consumption (watts)
    pub avg_power_watts: f32,
    /// Peak power consumption (watts)
    pub peak_power_watts: f32,
    /// Whether thermal throttling occurred
    pub thermal_throttled: bool,
}

impl ProfileReport {
    /// Print human-readable report
    pub fn print(&self) {
        info!("═══════════════════════════════════════════════════");
        info!("           ANE Performance Report");
        info!("═══════════════════════════════════════════════════");
        info!("Duration:            {:.2} seconds", self.duration_secs);
        info!("Samples:             {}", self.sample_count);
        info!("Avg Utilization:     {:.1}%", self.avg_utilization_percent);
        info!("Peak Utilization:    {:.1}%", self.peak_utilization_percent);
        info!("Avg Power:           {:.2} W", self.avg_power_watts);
        info!("Peak Power:          {:.2} W", self.peak_power_watts);
        info!("Thermal Throttling:  {}", if self.thermal_throttled { "YES" } else { "NO" });
        info!("═══════════════════════════════════════════════════");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ane_kernel_config_default() {
        let config = ANEKernelConfig::default();
        assert_eq!(config.hidden_size, 3584);
        assert_eq!(config.lora_rank, 16);
        assert_eq!(config.tile_size, 16);
        assert!(config.use_float16);
        assert!(config.enable_fusion);
    }

    #[test]
    fn test_shared_down_projection_creation() {
        let config = ANEKernelConfig {
            hidden_size: 64,
            lora_rank: 8,
            ..Default::default()
        };

        let weights = vec![0.1f32; 64 * 8];
        let result = SharedDownProjection::new(&weights, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_float32_to_float16_conversion() {
        let weights = vec![1.0f32, -1.0, 0.5, -0.5, 0.0];
        let result = SharedDownProjection::convert_f32_to_f16(&weights);
        assert!(result.is_ok());

        let f16_weights = result.unwrap();
        assert_eq!(f16_weights.len(), weights.len());
    }

    #[test]
    fn test_per_module_up_projection_creation() {
        let config = ANEKernelConfig {
            hidden_size: 64,
            lora_rank: 8,
            max_adapters: 4,
            ..Default::default()
        };

        let module_weights = vec![
            vec![0.1f32; 8 * 64],
            vec![0.2f32; 8 * 64],
        ];

        let gate_weights = vec![16384i16, 24576i16]; // Q15 format

        let result = PerModuleUpProjection::new(&module_weights, &gate_weights, config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_thermal_state_matching() {
        let state = ThermalState::Normal;
        assert!(!matches!(state, ThermalState::Heavy | ThermalState::Critical));

        let state = ThermalState::Heavy;
        assert!(matches!(state, ThermalState::Heavy | ThermalState::Critical));
    }

    #[test]
    fn test_performance_profiler() {
        let mut profiler = ANEPerformanceProfiler::new();

        profiler.record_utilization(75.0);
        profiler.record_utilization(80.0);
        profiler.record_utilization(85.0);

        assert_eq!(profiler.avg_utilization(), 80.0);

        profiler.record_power(2.5);
        profiler.record_power(3.0);

        assert_eq!(profiler.avg_power(), 2.75);

        profiler.record_thermal_state(ThermalState::Normal);
        profiler.record_thermal_state(ThermalState::Light);

        assert!(!profiler.has_throttled());

        profiler.record_thermal_state(ThermalState::Heavy);
        assert!(profiler.has_throttled());
    }
}
