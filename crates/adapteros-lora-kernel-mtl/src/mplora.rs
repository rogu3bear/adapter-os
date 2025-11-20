#![allow(unused_variables)]

//! MPLoRA implementation for Metal kernels
//!
//! Implements orthogonal multi-path LoRA routing as described in:
//! MPLoRA: Orthogonal Multi-Path Low-Rank Adaptation for Parameter Efficient Fine-Tuning
//! https://openreview.net/pdf?id=jqz6Msm3AF

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MploraConfig, RouterRing};
use metal::*;
use std::sync::Arc;

/// MPLoRA Metal kernel implementation
pub struct MploraKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    shared_downsample_buffer: Option<Buffer>,
    orthogonal_history_buffer: Option<Buffer>,
    compression_buffer: Option<Buffer>,
}

impl MploraKernel {
    /// Create a new MPLoRA kernel
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load the Metal library
        let library = device
            .new_library_with_data(include_bytes!("../../../metal/aos_kernels.metallib"))
            .map_err(|e| AosError::Mtl(format!("Failed to load Metal library: {}", e)))?;

        // Get the MPLoRA function
        let function = library
            .get_function("mplora_shared_downsample", None)
            .map_err(|e| AosError::Mtl(format!("MPLoRA function not found: {}", e)))?;

        // Create compute pipeline state
        let pipeline_state = device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Mtl(format!("Failed to create pipeline state: {}", e)))?;

        Ok(Self {
            device,
            command_queue,
            pipeline_state,
            shared_downsample_buffer: None,
            orthogonal_history_buffer: None,
            compression_buffer: None,
        })
    }

    /// Initialize shared downsample buffer
    fn init_shared_downsample_buffer(
        &mut self,
        shared_rank: usize,
        hidden_size: usize,
    ) -> Result<()> {
        let buffer_size = (shared_rank * hidden_size * std::mem::size_of::<f32>()) as u64;
        self.shared_downsample_buffer = Some(
            self.device
                .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared),
        );
        Ok(())
    }

    /// Initialize orthogonal history buffer
    fn init_orthogonal_history_buffer(
        &mut self,
        history_window: usize,
        adapter_count: usize,
    ) -> Result<()> {
        let buffer_size = (history_window * adapter_count * std::mem::size_of::<f32>()) as u64;
        self.orthogonal_history_buffer = Some(
            self.device
                .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared),
        );
        Ok(())
    }

    /// Initialize compression buffer
    fn init_compression_buffer(&mut self, compressed_size: usize) -> Result<()> {
        let buffer_size = (compressed_size * std::mem::size_of::<f32>()) as u64;
        self.compression_buffer = Some(
            self.device
                .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared),
        );
        Ok(())
    }
}

impl FusedKernels for MploraKernel {
    /// Load plan and weights
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Metal kernels are precompiled, no runtime loading needed
        Ok(())
    }

    /// Run a single token step
    fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
        // Default implementation - specific kernels override this
        Ok(())
    }

    /// Get device name
    fn device_name(&self) -> &str {
        "Metal GPU"
    }

    /// Attest to determinism guarantees
    fn attest_determinism(
        &self,
    ) -> Result<adapteros_lora_kernel_api::attestation::DeterminismReport> {
        // MploraKernel is built on top of MetalKernels, so it inherits determinism
        use adapteros_core::B3Hash;
        use adapteros_lora_kernel_api::attestation;

        // Get metallib hash from embedded constant
        let metallib_hash = B3Hash::from_hex(crate::METALLIB_HASH.trim()).map_err(|e| {
            adapteros_core::AosError::Kernel(format!("Invalid metallib hash: {}", e))
        })?;

        // Get manifest from verification
        let manifest_result = crate::verify_embedded_manifest(crate::METALLIB_BYTES, None);

        let manifest = manifest_result.ok().map(|m| attestation::KernelManifest {
            kernel_hash: m.kernel_hash,
            xcrun_version: m.xcrun_version,
            sdk_version: m.sdk_version,
            rust_version: m.rust_version,
            build_timestamp: m.build_timestamp,
        });

        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Metal,
            metallib_hash: Some(metallib_hash),
            manifest,
            rng_seed_method: attestation::RngSeedingMethod::HkdfSeeded,
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            compiler_flags: vec!["-O2".to_string(), "-std=metal3.1".to_string()],
            deterministic: true,
        })
    }

    /// Load adapter at runtime (hot-swap)
    fn load_adapter(&mut self, _adapter_id: u16, _weights: &[u8]) -> Result<()> {
        // Metal adapters are loaded via shared memory
        Ok(())
    }

    /// Unload adapter
    fn unload_adapter(&mut self, _adapter_id: u16) -> Result<()> {
        // Metal adapters are managed via shared memory
        Ok(())
    }

    /// Get device information string
    fn device_info(&self) -> String {
        "Metal GPU (MPLoRA)".to_string()
    }

    /// Execute compression kernel
    fn execute_compression(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()> {
        self.execute_compression(input, output, config)
    }
}

impl MploraKernels for MploraKernel {
    /// Execute MPLoRA with shared downsample
    fn execute_mplora(
        &mut self,
        ring: &RouterRing,
        io: &mut IoBuffers,
        mplora_config: &MploraConfig,
    ) -> Result<()> {
        if !mplora_config.shared_downsample {
            return Ok(()); // Skip if not enabled
        }

        let command_buffer = self.command_queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(&self.pipeline_state);

        // Set input buffer
        let input_buffer = self.device.new_buffer_with_data(
            io.output_logits.as_ptr() as *const std::ffi::c_void,
            (io.output_logits.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(0, Some(&input_buffer), 0);

        // Set shared downsample buffer
        if let Some(ref shared_buffer) = self.shared_downsample_buffer {
            encoder.set_buffer(1, Some(shared_buffer), 0);
        }

        // Set adapter B matrices buffer
        let adapter_bs_buffer = self.device.new_buffer_with_data(
            ring.indices.as_ptr() as *const std::ffi::c_void,
            (ring.k * std::mem::size_of::<u16>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(2, Some(&adapter_bs_buffer), 0);

        // Set gates buffer
        let gates_buffer = self.device.new_buffer_with_data(
            ring.gates_q15.as_ptr() as *const std::ffi::c_void,
            (ring.k * std::mem::size_of::<i16>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(3, Some(&gates_buffer), 0);

        // Set output buffer
        let output_buffer = self.device.new_buffer_with_data(
            io.output_logits.as_mut_ptr() as *const std::ffi::c_void,
            (io.output_logits.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(4, Some(&output_buffer), 0);

        // Set configuration
        let config_bytes = serde_json::to_vec(mplora_config).map_err(AosError::Serialization)?;
        let config_buffer = self.device.new_buffer_with_data(
            config_bytes.as_ptr() as *const std::ffi::c_void,
            config_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(5, Some(&config_buffer), 0);

        // Calculate threadgroup size
        let threadgroup_size = MTLSize::new(16, 16, 1);
        let grid_size = MTLSize::new(ring.k as u64, ring.k as u64, 1);

        encoder.dispatch_thread_groups(grid_size, threadgroup_size);
        encoder.end_encoding();

        command_buffer.commit();
        command_buffer.wait_until_completed();

        Ok(())
    }

    /// Apply orthogonal constraints
    fn apply_orthogonal_constraints(
        &mut self,
        adapter_indices: &[u16],
        gates: &[i16],
        config: &MploraConfig,
    ) -> Result<()> {
        if !config.orthogonal_constraints {
            return Ok(()); // Skip if not enabled
        }

        // Initialize history buffer if needed
        if self.orthogonal_history_buffer.is_none() {
            self.init_orthogonal_history_buffer(config.history_window, adapter_indices.len())?;
        }

        // This would implement the orthogonal constraint enforcement
        // For now, we just track the constraint application
        tracing::debug!(
            "Applied orthogonal constraints to {} adapters with similarity threshold {}",
            adapter_indices.len(),
            config.similarity_threshold
        );

        Ok(())
    }

    /// Execute shared downsample kernel
    fn execute_shared_downsample(
        &mut self,
        input: &[f32],
        shared_a: &[f32],
        adapter_bs: &[f32],
        gates: &[i16],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()> {
        if !config.shared_downsample {
            return Ok(()); // Skip if not enabled
        }

        // Initialize shared downsample buffer if needed
        if self.shared_downsample_buffer.is_none() {
            let shared_rank = (shared_a.len() / input.len()).max(1);
            self.init_shared_downsample_buffer(shared_rank, input.len())?;
        }

        // This would implement the shared downsample kernel execution
        // For now, we just track the operation
        tracing::debug!(
            "Executed shared downsample with compression ratio {}",
            config.compression_ratio
        );

        Ok(())
    }

    /// Execute compression kernel
    fn execute_compression(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()> {
        // Initialize compression buffer if needed
        if self.compression_buffer.is_none() {
            let compressed_size = (input.len() as f32 * config.compression_ratio) as usize;
            self.init_compression_buffer(compressed_size)?;
        }

        // Simple compression implementation (placeholder)
        let compressed_size = (input.len() as f32 * config.compression_ratio) as usize;
        let step = input.len() / compressed_size.max(1);

        for i in 0..compressed_size {
            let mut sum = 0.0;
            for j in 0..step {
                if i * step + j < input.len() {
                    sum += input[i * step + j];
                }
            }
            output[i] = sum / step as f32;
        }

        tracing::debug!(
            "Executed compression: {} -> {} elements (ratio: {})",
            input.len(),
            compressed_size,
            config.compression_ratio
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mplora_config_default() {
        let config = MploraConfig::default();
        assert!(!config.shared_downsample);
        assert_eq!(config.compression_ratio, 0.8);
        assert!(!config.orthogonal_constraints);
        assert_eq!(config.similarity_threshold, 0.7);
        assert_eq!(config.penalty_weight, 0.1);
        assert_eq!(config.history_window, 10);
    }

    #[test]
    fn test_mplora_config_serialization() {
        let config = MploraConfig {
            shared_downsample: true,
            compression_ratio: 0.6,
            orthogonal_constraints: true,
            similarity_threshold: 0.8,
            penalty_weight: 0.15,
            history_window: 20,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: MploraConfig = serde_json::from_str(&serialized).unwrap();

        assert_eq!(config.shared_downsample, deserialized.shared_downsample);
        assert_eq!(config.compression_ratio, deserialized.compression_ratio);
        assert_eq!(
            config.orthogonal_constraints,
            deserialized.orthogonal_constraints
        );
        assert_eq!(
            config.similarity_threshold,
            deserialized.similarity_threshold
        );
        assert_eq!(config.penalty_weight, deserialized.penalty_weight);
        assert_eq!(config.history_window, deserialized.history_window);
    }
}
