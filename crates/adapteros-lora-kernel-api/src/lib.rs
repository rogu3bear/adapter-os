//! Safe API for fused kernels

use adapteros_core::Result;
use serde::{Deserialize, Serialize};

pub mod attestation;

/// Ring buffer for router decisions (Q15 gates)
#[derive(Debug, Clone)]
pub struct RouterRing {
    /// Adapter indices (up to K=8)
    pub indices: Vec<u16>,
    /// Q15 quantized gates
    pub gates_q15: Vec<i16>,
    /// Token position
    pub position: usize,
}

impl RouterRing {
    pub fn new(k: usize) -> Self {
        Self {
            indices: vec![0; k],
            gates_q15: vec![0; k],
            position: 0,
        }
    }

    pub fn set(&mut self, indices: &[u16], gates: &[i16]) {
        self.indices[..indices.len()].copy_from_slice(indices);
        self.gates_q15[..gates.len()].copy_from_slice(gates);
    }
}

/// IO buffers for kernel execution
pub struct IoBuffers {
    pub input_ids: Vec<u32>,
    pub output_logits: Vec<f32>,
    pub position: usize,
}

impl IoBuffers {
    pub fn new(vocab_size: usize) -> Self {
        Self {
            input_ids: Vec::new(),
            output_logits: vec![0.0; vocab_size],
            position: 0,
        }
    }
}

/// Trait for fused kernel implementations
pub trait FusedKernels: Send {
    /// Load plan and weights
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;

    /// Run a single token step
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;

    /// Get device name
    fn device_name(&self) -> &str;

    /// Attest to determinism guarantees of this backend
    ///
    /// Returns a DeterminismReport containing metallib hash, RNG seeding method,
    /// floating-point mode, compiler flags, and overall deterministic attestation.
    ///
    /// This method is called during backend initialization and before serving
    /// to validate that the backend meets determinism requirements.
    fn attest_determinism(&self) -> Result<attestation::DeterminismReport>;

    /// Load adapter at runtime (hot-swap)
    ///
    /// Default implementation returns error for backends that don't support hot-swap
    fn load_adapter(&mut self, _id: u16, _weights: &[u8]) -> Result<()> {
        Err(adapteros_core::AosError::Kernel(
            "Hot-swap not supported by this backend".to_string(),
        ))
    }

    /// Unload adapter at runtime (hot-swap)
    ///
    /// Default implementation returns error for backends that don't support hot-swap
    fn unload_adapter(&mut self, _id: u16) -> Result<()> {
        Err(adapteros_core::AosError::Kernel(
            "Hot-swap not supported by this backend".to_string(),
        ))
    }
}

/// Mock kernels implementation for testing
pub struct MockKernels {
    device_name: String,
}

impl MockKernels {
    /// Create a new mock kernels instance
    pub fn new() -> Self {
        Self {
            device_name: "Mock Kernels (Test)".to_string(),
        }
    }
}

impl FusedKernels for MockKernels {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Mock implementation - no-op
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Mock implementation - generate deterministic logits for testing
        for (i, logit) in io.output_logits.iter_mut().enumerate() {
            *logit = (i as f32 * 0.001) % 1.0; // Deterministic pattern
        }

        io.position += 1;
        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // Mock kernels are deterministic for testing purposes
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Mock,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::FixedSeed(0),
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: true,
        })
    }
}

impl Default for MockKernels {
    fn default() -> Self {
        Self::new()
    }
}

/// Impl FusedKernels for Box<dyn FusedKernels> to enable dynamic dispatch
impl FusedKernels for Box<dyn FusedKernels> {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        (**self).load(plan_bytes)
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        (**self).run_step(ring, io)
    }

    fn device_name(&self) -> &str {
        (**self).device_name()
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        (**self).attest_determinism()
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        (**self).load_adapter(id, weights)
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        (**self).unload_adapter(id)
    }
}

/// MPLoRA configuration for kernels
/// Reference: https://openreview.net/pdf?id=jqz6Msm3AF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraConfig {
    pub shared_downsample: bool,
    pub compression_ratio: f32,
    pub orthogonal_constraints: bool,
    pub similarity_threshold: f32,
    pub penalty_weight: f32,
    pub history_window: usize,
}

impl Default for MploraConfig {
    fn default() -> Self {
        Self {
            shared_downsample: false,
            compression_ratio: 0.8,
            orthogonal_constraints: false,
            similarity_threshold: 0.7,
            penalty_weight: 0.1,
            history_window: 10,
        }
    }
}

/// Extended kernel trait with MPLoRA support
pub trait MploraKernels: FusedKernels {
    /// Execute MPLoRA with shared downsample
    fn execute_mplora(
        &mut self,
        ring: &RouterRing,
        io: &mut IoBuffers,
        mplora_config: &MploraConfig,
    ) -> Result<()>;

    /// Apply orthogonal constraints
    fn apply_orthogonal_constraints(
        &mut self,
        adapter_indices: &[u16],
        gates: &[i16],
        config: &MploraConfig,
    ) -> Result<()>;

    /// Execute shared downsample kernel
    fn execute_shared_downsample(
        &mut self,
        input: &[f32],
        shared_a: &[f32],
        adapter_bs: &[f32],
        gates: &[i16],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()>;

    /// Execute compression kernel
    fn execute_compression(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()>;
}
