//! Safe API for fused kernels

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub mod attestation;

/// Maximum number of adapters per routing step (K-sparse constraint)
pub const MAX_ADAPTERS_PER_STEP: usize = 8;

/// Ring buffer for router decisions (Q15 gates)
///
/// # Contract Invariants (PRD 6)
/// 1. `indices.len() == gates_q15.len()` for every ring
/// 2. `indices` MUST be sorted ascending by adapter index
/// 3. `indices.len()` MUST NOT exceed MAX_ADAPTERS_PER_STEP
/// 4. `gates_q15` MUST be in Q15 range: [-32768, 32767]
/// 5. Router MUST enforce K <= MAX_ADAPTERS_PER_STEP
/// 6. All backends MUST consume RouterRing as defined and MUST NOT reinterpret layout
///
/// # Design Notes
/// - `position: u64` chosen over `usize` for:
///   - Platform-independent serialization (no 32/64-bit differences)
///   - Support for sequences > 4B tokens (future-proof)
///   - FFI-safe fixed-size type (no pointer-width dependencies)
/// - `SmallVec<[T; 8]>` provides stack allocation for K≤8 (zero heap overhead)
#[derive(Debug, Clone)]
pub struct RouterRing {
    /// Adapter indices (up to K=8), MUST be sorted ascending
    pub indices: SmallVec<[u16; MAX_ADAPTERS_PER_STEP]>,
    /// Q15 quantized gates in range [-32768, 32767]
    pub gates_q15: SmallVec<[i16; MAX_ADAPTERS_PER_STEP]>,
    /// Token position: u64 for platform-independent, large-sequence support
    pub position: u64,
}

impl RouterRing {
    /// Create a new empty RouterRing
    pub fn new() -> Self {
        Self {
            indices: SmallVec::new(),
            gates_q15: SmallVec::new(),
            position: 0,
        }
    }

    /// Create a RouterRing with pre-allocated capacity
    pub fn with_capacity(k: usize) -> Result<Self> {
        if k > MAX_ADAPTERS_PER_STEP {
            return Err(AosError::Validation(format!(
                "K={} exceeds MAX_ADAPTERS_PER_STEP={}",
                k, MAX_ADAPTERS_PER_STEP
            )));
        }
        Ok(Self {
            indices: SmallVec::with_capacity(k),
            gates_q15: SmallVec::with_capacity(k),
            position: 0,
        })
    }

    /// Set indices and gates with validation
    ///
    /// # Errors
    /// Returns error if:
    /// - Lengths don't match
    /// - Exceeds MAX_ADAPTERS_PER_STEP
    /// - Indices not sorted ascending
    /// - Gates outside Q15 range
    pub fn set(&mut self, indices: &[u16], gates: &[i16]) -> Result<()> {
        // Validate contract invariants
        if indices.len() != gates.len() {
            return Err(AosError::Validation(format!(
                "RouterRing length mismatch: indices={}, gates={}",
                indices.len(),
                gates.len()
            )));
        }

        if indices.len() > MAX_ADAPTERS_PER_STEP {
            return Err(AosError::Validation(format!(
                "RouterRing length {} exceeds MAX_ADAPTERS_PER_STEP={}",
                indices.len(),
                MAX_ADAPTERS_PER_STEP
            )));
        }

        // Check sorted ascending
        if !Self::is_sorted_ascending(indices) {
            return Err(AosError::Validation(
                "RouterRing indices must be sorted ascending".to_string(),
            ));
        }

        // Check Q15 range (i16 is inherently in range, but document the invariant)
        // Q15 format: [-32768, 32767] is the full i16 range

        self.indices.clear();
        self.indices.extend_from_slice(indices);
        self.gates_q15.clear();
        self.gates_q15.extend_from_slice(gates);

        Ok(())
    }

    /// Validate all contract invariants
    ///
    /// # Errors
    /// Returns error if any invariant is violated
    pub fn validate_invariants(&self) -> Result<()> {
        // Invariant 1: lengths match
        if self.indices.len() != self.gates_q15.len() {
            return Err(AosError::Validation(format!(
                "RouterRing length mismatch: indices={}, gates={}",
                self.indices.len(),
                self.gates_q15.len()
            )));
        }

        // Invariant 3: not exceed max
        if self.indices.len() > MAX_ADAPTERS_PER_STEP {
            return Err(AosError::Validation(format!(
                "RouterRing length {} exceeds MAX_ADAPTERS_PER_STEP={}",
                self.indices.len(),
                MAX_ADAPTERS_PER_STEP
            )));
        }

        // Invariant 2: sorted ascending
        if !Self::is_sorted_ascending(&self.indices) {
            return Err(AosError::Validation(
                "RouterRing indices not sorted ascending".to_string(),
            ));
        }

        // Invariant 4: Q15 range (i16 inherently satisfies this)
        // Gates are i16, so they're always in [-32768, 32767]

        Ok(())
    }

    /// Check if indices are sorted in ascending order
    fn is_sorted_ascending(indices: &[u16]) -> bool {
        indices.windows(2).all(|w| w[0] < w[1])
            || indices.len() <= 1 // Empty or single element is sorted
    }

    /// Get the number of adapters in this ring
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Check if ring is empty
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Get memory layout info for testing/debugging
    pub fn layout_info(&self) -> RouterRingLayout {
        RouterRingLayout {
            indices_len: self.indices.len(),
            gates_len: self.gates_q15.len(),
            position: self.position,
            indices_size: std::mem::size_of_val(self.indices.as_slice()),
            gates_size: std::mem::size_of_val(self.gates_q15.as_slice()),
            total_size: std::mem::size_of::<Self>(),
        }
    }
}

impl Default for RouterRing {
    fn default() -> Self {
        Self::new()
    }
}

/// Layout information for RouterRing (for golden tests)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterRingLayout {
    pub indices_len: usize,
    pub gates_len: usize,
    pub position: u64,
    pub indices_size: usize,
    pub gates_size: usize,
    pub total_size: usize,
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
pub trait FusedKernels: Send + Sync {
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

    /// Verify GPU adapter buffers and compute fingerprint
    ///
    /// Samples buffer contents at checkpoints (first/last/mid 4KB) and returns
    /// a fingerprint for integrity verification. This enables cross-layer validation
    /// without full GPU-to-CPU buffer readback.
    ///
    /// # Arguments
    /// * `id` - Adapter ID to verify
    ///
    /// # Returns
    /// * Buffer size in bytes
    /// * Checkpoint samples (first 4KB, last 4KB, mid 4KB)
    ///
    /// Default implementation returns error for backends without GPU verification
    fn verify_adapter_buffers(&self, _id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
        Err(adapteros_core::AosError::Kernel(
            "GPU buffer verification not supported by this backend".to_string(),
        ))
    }

    /// Store GPU buffer fingerprint for adapter
    ///
    /// Stores a BLAKE3 hash of GPU buffer checkpoint samples for later verification.
    /// Used after adapter load to establish baseline.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Buffer size in bytes
    /// * `checkpoint_hash_hex` - BLAKE3 hash of checkpoint samples as hex string
    ///
    /// Default implementation is no-op for backends without GPU tracking
    fn store_gpu_fingerprint(&mut self, _id: u16, _buffer_size: u64, _checkpoint_hash_hex: &str) {
        // No-op for backends without VRAM tracking
    }

    /// Verify GPU buffer fingerprint matches stored baseline
    ///
    /// Compares current GPU buffer fingerprint against stored baseline.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Current buffer size
    /// * `checkpoint_hash_hex` - Current BLAKE3 hash as hex string
    ///
    /// # Returns
    /// * `Ok(true)` - Fingerprint matches baseline
    /// * `Ok(false)` - No baseline stored yet (first verification)
    /// * `Err(msg)` - Fingerprint mismatch
    ///
    /// Default implementation returns Ok(true) for backends without GPU tracking
    fn verify_gpu_fingerprint(
        &self,
        _id: u16,
        _buffer_size: u64,
        _checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        Ok(true) // No verification for non-GPU backends
    }

    /// Check if memory footprint is within adaptive baseline tolerance
    ///
    /// Uses 2σ tolerance with adaptive baseline learning.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Current buffer size
    ///
    /// # Returns
    /// * within_tolerance: bool
    /// * z_score: f64
    /// * baseline_stats: Option<(mean, stddev, sample_count)>
    ///
    /// Default implementation returns (true, 0.0, None) for backends without tracking
    fn check_memory_footprint(
        &self,
        _id: u16,
        _buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (true, 0.0, None) // No anomaly detection for non-GPU backends
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

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // PRD 6: MockKernels validates contract to catch violations in all tests
        ring.validate_invariants()?;

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

    fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
        (**self).verify_adapter_buffers(id)
    }

    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) {
        (**self).store_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
    }

    fn verify_gpu_fingerprint(
        &self,
        id: u16,
        buffer_size: u64,
        checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        (**self).verify_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
    }

    fn check_memory_footprint(
        &self,
        id: u16,
        buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (**self).check_memory_footprint(id, buffer_size)
    }
}

/// Impl FusedKernels for Box<dyn FusedKernels + Send + Sync> to enable dynamic dispatch with explicit bounds
impl FusedKernels for Box<dyn FusedKernels + Send + Sync> {
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

    fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
        (**self).verify_adapter_buffers(id)
    }

    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) {
        (**self).store_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
    }

    fn verify_gpu_fingerprint(
        &self,
        id: u16,
        buffer_size: u64,
        checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        (**self).verify_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
    }

    fn check_memory_footprint(
        &self,
        id: u16,
        buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (**self).check_memory_footprint(id, buffer_size)
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
