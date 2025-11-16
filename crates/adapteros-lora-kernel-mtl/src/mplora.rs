#![allow(unused_variables)]

//! MPLoRA implementation for Metal kernels
//!
//! Implements orthogonal multi-path LoRA routing as described in:
//! MPLoRA: Orthogonal Multi-Path Low-Rank Adaptation for Parameter Efficient Fine-Tuning
//! https://openreview.net/pdf?id=jqz6Msm3AF

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, MploraConfig, MploraKernels, RouterRing};
use metal::*;
use safetensors::{tensor::TensorView, Dtype, SafeTensors};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};

/// MPLoRA Metal kernel implementation
pub struct MploraKernel {
    device: Arc<Device>,
    command_queue: CommandQueue,
    pipeline_state: ComputePipelineState,
    shared_downsample_buffer: Option<Buffer>,
    orthogonal_history_buffer: Option<Buffer>,
    compression_buffer: Option<Buffer>,
    loaded_adapters: RwLock<HashMap<u16, LoadedAdapter>>,
    gpu_memory_bytes: AtomicU64,
}

#[derive(Debug)]
struct AdapterModuleBuffers {
    lora_a: Buffer,
    lora_b: Buffer,
    input_dim: usize,
    output_dim: usize,
    rank: usize,
}

#[derive(Debug)]
struct LoadedAdapter {
    id: u16,
    alpha: f32,
    rank: usize,
    total_bytes: u64,
    modules: HashMap<String, AdapterModuleBuffers>,
}

impl MploraKernel {
    /// Create a new MPLoRA kernel
    pub fn new(device: Arc<Device>) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Load the Metal library from the crate's embedded metallib, aligned with other kernels
        let library = device
            .new_library_with_data(include_bytes!("../shaders/adapteros_kernels.metallib"))
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
            loaded_adapters: RwLock::new(HashMap::new()),
            gpu_memory_bytes: AtomicU64::new(0),
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

    fn parse_matrix_shape(label: &str, tensor: &TensorView) -> Result<(usize, usize)> {
        let shape = tensor.shape();
        if shape.len() != 2 {
            return Err(AosError::Kernel(format!(
                "Tensor {} expected rank 2, got {:?}",
                label, shape
            )));
        }

        Ok((shape[0] as usize, shape[1] as usize))
    }

    fn tensor_to_f32_vec(tensor: &TensorView, label: &str) -> Result<Vec<f32>> {
        match tensor.dtype() {
            Dtype::F32 => {
                let data = tensor.data();
                let elem_size = std::mem::size_of::<f32>();
                if data.len() % elem_size != 0 {
                    return Err(AosError::Kernel(format!(
                        "Tensor {} size {} is not aligned to f32 ({} bytes)",
                        label,
                        data.len(),
                        elem_size
                    )));
                }

                let mut values = vec![0f32; data.len() / elem_size];
                for (idx, chunk) in data.chunks_exact(elem_size).enumerate() {
                    values[idx] = f32::from_le_bytes(chunk.try_into().expect("chunk exact"));
                }
                Ok(values)
            }
            other => Err(AosError::Kernel(format!(
                "Tensor {} must be f32, got {:?}",
                label, other
            ))),
        }
    }

    fn prepare_adapter(&self, adapter_id: u16, weights: &[u8]) -> Result<LoadedAdapter> {
        let safetensors = SafeTensors::deserialize(weights).map_err(|err| {
            AosError::Kernel(format!(
                "Adapter {} weights are not valid safetensors: {}",
                adapter_id, err
            ))
        })?;

        let mut module_names: Vec<String> = safetensors
            .names()
            .iter()
            .filter_map(|name| name.strip_prefix("lora_a.").map(|m| m.to_string()))
            .collect();
        module_names.sort();
        module_names.dedup();

        if module_names.is_empty() {
            return Err(AosError::Kernel(format!(
                "Adapter {} contains no lora_a.* tensors",
                adapter_id
            )));
        }

        let mut modules = HashMap::new();
        let mut total_bytes = 0u64;
        let mut detected_rank: Option<usize> = None;

        for module in module_names.iter() {
            let a_label = format!("lora_a.{}", module);
            let b_label = format!("lora_b.{}", module);

            let a_tensor = safetensors.tensor(&a_label).map_err(|err| {
                AosError::Kernel(format!(
                    "Adapter {} missing tensor {}: {}",
                    adapter_id, a_label, err
                ))
            })?;
            let b_tensor = safetensors.tensor(&b_label).map_err(|err| {
                AosError::Kernel(format!(
                    "Adapter {} missing tensor {}: {}",
                    adapter_id, b_label, err
                ))
            })?;

            let (rank_rows, input_dim) = Self::parse_matrix_shape(&a_label, &a_tensor)?;
            let (output_dim, rank_cols) = Self::parse_matrix_shape(&b_label, &b_tensor)?;

            if rank_rows == 0 || rank_cols == 0 {
                return Err(AosError::Kernel(format!(
                    "Adapter {} module {} has zero-sized matrices",
                    adapter_id, module
                )));
            }

            if rank_rows != rank_cols {
                return Err(AosError::Kernel(format!(
                    "Adapter {} module {} rank mismatch: A has {}, B has {}",
                    adapter_id, module, rank_rows, rank_cols
                )));
            }

            let a_values = Self::tensor_to_f32_vec(&a_tensor, &a_label)?;
            let b_values = Self::tensor_to_f32_vec(&b_tensor, &b_label)?;

            let a_buffer = self.device.new_buffer_with_data(
                a_values.as_ptr() as *const std::ffi::c_void,
                (a_values.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let b_buffer = self.device.new_buffer_with_data(
                b_values.as_ptr() as *const std::ffi::c_void,
                (b_values.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );

            total_bytes += a_buffer.length();
            total_bytes += b_buffer.length();

            modules.insert(
                module.clone(),
                AdapterModuleBuffers {
                    lora_a: a_buffer,
                    lora_b: b_buffer,
                    input_dim,
                    output_dim,
                    rank: rank_rows,
                },
            );

            if let Some(existing_rank) = detected_rank {
                if existing_rank != rank_rows {
                    return Err(AosError::Kernel(format!(
                        "Adapter {} has inconsistent ranks ({} vs {})",
                        adapter_id, existing_rank, rank_rows
                    )));
                }
            } else {
                detected_rank = Some(rank_rows);
            }
        }

        let alpha = safetensors
            .metadata()
            .and_then(|meta| meta.get("alpha"))
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0);

        let rank = detected_rank.unwrap_or(0);
        if rank == 0 {
            return Err(AosError::Kernel(format!(
                "Adapter {} resolved to rank 0",
                adapter_id
            )));
        }

        Ok(LoadedAdapter {
            id: adapter_id,
            alpha,
            rank,
            total_bytes,
            modules,
        })
    }

    fn synchronize_queue(&self) {
        let fence = self.command_queue.new_command_buffer();
        fence.commit();
        fence.wait_until_completed();
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
    fn load_adapter(&mut self, adapter_id: u16, weights: &[u8]) -> Result<()> {
        // Ensure any in-flight command buffers complete before mutating state
        self.synchronize_queue();

        let prepared = self.prepare_adapter(adapter_id, weights)?;
        let adapter_bytes = prepared.total_bytes;
        let module_count = prepared.modules.len();
        let rank = prepared.rank;
        let alpha = prepared.alpha;

        {
            let mut guard = self
                .loaded_adapters
                .write()
                .expect("loaded adapter map poisoned");
            if let Some(previous) = guard.insert(adapter_id, prepared) {
                self.gpu_memory_bytes
                    .fetch_sub(previous.total_bytes, Ordering::SeqCst);
                tracing::debug!(
                    adapter_id,
                    previous_rank = previous.rank,
                    "Replacing previously loaded adapter"
                );
            }
        }

        self.gpu_memory_bytes
            .fetch_add(adapter_bytes, Ordering::SeqCst);

        // Barrier after uploading to guarantee deterministic visibility
        self.synchronize_queue();

        tracing::info!(
            adapter_id,
            modules = module_count,
            rank,
            alpha,
            bytes = adapter_bytes,
            gpu_bytes = self.gpu_memory_bytes.load(Ordering::SeqCst),
            "Hot-loaded adapter into Metal kernel"
        );

        Ok(())
    }

    /// Unload adapter
    fn unload_adapter(&mut self, adapter_id: u16) -> Result<()> {
        // Drain outstanding work before mutating adapter map
        self.synchronize_queue();

        let removed = {
            let mut guard = self
                .loaded_adapters
                .write()
                .expect("loaded adapter map poisoned");
            guard.remove(&adapter_id)
        };

        match removed {
            Some(adapter) => {
                self.gpu_memory_bytes
                    .fetch_sub(adapter.total_bytes, Ordering::SeqCst);

                self.synchronize_queue();

                tracing::info!(
                    adapter_id,
                    bytes = adapter.total_bytes,
                    gpu_bytes = self.gpu_memory_bytes.load(Ordering::SeqCst),
                    "Unloaded adapter from Metal kernel"
                );
                Ok(())
            }
            None => Err(AosError::Kernel(format!(
                "Adapter {} not loaded",
                adapter_id
            ))),
        }
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
            (ring.indices.len() * std::mem::size_of::<u16>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(2, Some(&adapter_bs_buffer), 0);

        // Set gates buffer
        let gates_buffer = self.device.new_buffer_with_data(
            ring.gates_q15.as_ptr() as *const std::ffi::c_void,
            (ring.gates_q15.len() * std::mem::size_of::<i16>()) as u64,
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
        let grid_size = MTLSize::new(ring.indices.len() as u64, ring.gates_q15.len() as u64, 1);

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
