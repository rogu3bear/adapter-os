//! Safe API for fused kernels

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

pub mod attestation;

/// Maximum adapters per routing step (PRD 6 contract)
pub const MAX_ADAPTERS_PER_STEP: usize = 8;

/// Ring buffer for router decisions (Q15 gates)
///
/// # Invariants (PRD 6)
/// 1. `indices.len() == gates_q15.len()` (1:1 mapping)
/// 2. `indices` MUST be sorted ascending
/// 3. `indices.len() <= MAX_ADAPTERS_PER_STEP`
/// 4. `gates_q15` values MUST be in Q15 range: [-32768, 32767]
/// 5. All backends MUST validate these invariants in debug builds
#[derive(Debug, Clone)]
pub struct RouterRing {
    /// Adapter indices (up to MAX_ADAPTERS_PER_STEP=8), sorted ascending
    pub indices: SmallVec<[u16; MAX_ADAPTERS_PER_STEP]>,
    /// Q15 quantized gates (range: [-32768, 32767])
    pub gates_q15: SmallVec<[i16; MAX_ADAPTERS_PER_STEP]>,
    /// Token position in sequence
    pub position: u64,
}

impl RouterRing {
    /// Create new RouterRing with capacity k (must be <= MAX_ADAPTERS_PER_STEP)
    ///
    /// # Panics
    /// Panics if k > MAX_ADAPTERS_PER_STEP
    pub fn new(k: usize) -> Self {
        assert!(
            k <= MAX_ADAPTERS_PER_STEP,
            "RouterRing k={} exceeds MAX_ADAPTERS_PER_STEP={}",
            k,
            MAX_ADAPTERS_PER_STEP
        );
        Self {
            indices: SmallVec::new(),
            gates_q15: SmallVec::new(),
            position: 0,
        }
    }

    /// Set indices and gates, validating invariants
    ///
    /// # Errors
    /// Returns error if:
    /// - indices.len() != gates.len()
    /// - indices.len() > MAX_ADAPTERS_PER_STEP
    /// - indices are not sorted ascending
    /// - any gate is outside Q15 range (though i16 guarantees this)
    pub fn set(&mut self, indices: &[u16], gates: &[i16]) -> Result<()> {
        // Invariant 1: lengths must match
        if indices.len() != gates.len() {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing invariant violated: indices.len()={} != gates.len()={}",
                indices.len(),
                gates.len()
            )));
        }

        // Invariant 3: must not exceed MAX_ADAPTERS_PER_STEP
        if indices.len() > MAX_ADAPTERS_PER_STEP {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing invariant violated: len={} exceeds MAX_ADAPTERS_PER_STEP={}",
                indices.len(),
                MAX_ADAPTERS_PER_STEP
            )));
        }

        // Invariant 2: indices must be sorted ascending
        if !indices.windows(2).all(|w| w[0] < w[1]) {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing invariant violated: indices not sorted ascending: {:?}",
                indices
            )));
        }

        // Invariant 4: Q15 range (i16 type already guarantees this, but we document it)
        // All i16 values are by definition in [-32768, 32767]

        self.indices.clear();
        self.indices.extend_from_slice(indices);
        self.gates_q15.clear();
        self.gates_q15.extend_from_slice(gates);

        Ok(())
    }

    /// Validate all invariants (for debug builds and tests)
    ///
    /// # Errors
    /// Returns error if any invariant is violated
    pub fn validate(&self) -> Result<()> {
        // Invariant 1: lengths match
        if self.indices.len() != self.gates_q15.len() {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing validation failed: indices.len()={} != gates.len()={}",
                self.indices.len(),
                self.gates_q15.len()
            )));
        }

        // Invariant 3: length limit
        if self.indices.len() > MAX_ADAPTERS_PER_STEP {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing validation failed: len={} exceeds MAX_ADAPTERS_PER_STEP={}",
                self.indices.len(),
                MAX_ADAPTERS_PER_STEP
            )));
        }

        // Invariant 2: sorted ascending
        if !self.indices.windows(2).all(|w| w[0] < w[1]) {
            return Err(adapteros_core::AosError::Kernel(format!(
                "RouterRing validation failed: indices not sorted: {:?}",
                self.indices.as_slice()
            )));
        }

        // Invariant 4: Q15 range (always satisfied for i16)
        // No validation needed - type system guarantees this

        Ok(())
    }

    /// Get length (number of active adapters)
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Check if empty (no adapters selected)
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PRD 6 Test: RouterRing golden layout (size, alignment)
    #[test]
    fn test_router_ring_layout() {
        use std::mem::{align_of, size_of};

        // RouterRing should have expected size and alignment
        let size = size_of::<RouterRing>();
        let align = align_of::<RouterRing>();

        // SmallVec<[u16; 8]> = 8 * 2 = 16 bytes + overhead
        // SmallVec<[i16; 8]> = 8 * 2 = 16 bytes + overhead
        // position: u64 = 8 bytes
        // SmallVec has 24-byte overhead (len, cap, union)
        // Total should be ~80 bytes on 64-bit systems

        println!("RouterRing size: {} bytes, align: {} bytes", size, align);

        // Ensure reasonable bounds
        assert!(size >= 48, "RouterRing too small: {} bytes", size);
        assert!(size <= 128, "RouterRing too large: {} bytes", size);
        assert_eq!(align, 8, "RouterRing alignment should be 8 bytes");
    }

    /// PRD 6 Test: Invariant 1 - lengths must match
    #[test]
    fn test_router_ring_length_mismatch() {
        let mut ring = RouterRing::new(4);
        let indices = vec![0, 1, 2];
        let gates = vec![100, 200]; // Mismatched length

        let result = ring.set(&indices, &gates);
        assert!(
            result.is_err(),
            "RouterRing should reject mismatched lengths"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("indices.len()=3 != gates.len()=2"));
    }

    /// PRD 6 Test: Invariant 2 - indices must be sorted ascending
    #[test]
    fn test_router_ring_unsorted_indices() {
        let mut ring = RouterRing::new(4);
        let indices = vec![0, 2, 1, 3]; // Not sorted
        let gates = vec![100, 200, 300, 400];

        let result = ring.set(&indices, &gates);
        assert!(
            result.is_err(),
            "RouterRing should reject unsorted indices"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not sorted ascending"));
    }

    /// PRD 6 Test: Invariant 3 - must not exceed MAX_ADAPTERS_PER_STEP
    #[test]
    fn test_router_ring_exceeds_max() {
        let mut ring = RouterRing::new(8);
        let indices: Vec<u16> = (0..9).collect(); // 9 > MAX_ADAPTERS_PER_STEP
        let gates: Vec<i16> = vec![100; 9];

        let result = ring.set(&indices, &gates);
        assert!(
            result.is_err(),
            "RouterRing should reject len > MAX_ADAPTERS_PER_STEP"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds MAX_ADAPTERS_PER_STEP"));
    }

    /// PRD 6 Test: Valid RouterRing with sorted indices
    #[test]
    fn test_router_ring_valid() {
        let mut ring = RouterRing::new(4);
        let indices = vec![0, 2, 5, 7]; // Sorted ascending
        let gates = vec![100, 200, 300, 400]; // Q15 range

        let result = ring.set(&indices, &gates);
        assert!(result.is_ok(), "Valid RouterRing should succeed");

        assert_eq!(ring.indices.as_slice(), &[0, 2, 5, 7]);
        assert_eq!(ring.gates_q15.as_slice(), &[100, 200, 300, 400]);
        assert_eq!(ring.len(), 4);
        assert!(!ring.is_empty());

        // Validate should pass
        assert!(ring.validate().is_ok());
    }

    /// PRD 6 Test: Empty RouterRing
    #[test]
    fn test_router_ring_empty() {
        let mut ring = RouterRing::new(4);
        let indices: Vec<u16> = vec![];
        let gates: Vec<i16> = vec![];

        let result = ring.set(&indices, &gates);
        assert!(result.is_ok(), "Empty RouterRing should be valid");

        assert_eq!(ring.len(), 0);
        assert!(ring.is_empty());
        assert!(ring.validate().is_ok());
    }

    /// PRD 6 Test: Q15 range (i16 type guarantees this)
    #[test]
    fn test_router_ring_q15_range() {
        let mut ring = RouterRing::new(3);
        let indices = vec![0, 1, 2];
        let gates = vec![-32768, 0, 32767]; // Full Q15 range

        let result = ring.set(&indices, &gates);
        assert!(result.is_ok(), "Q15 range should be valid");
        assert_eq!(ring.gates_q15.as_slice(), &[-32768, 0, 32767]);
    }

    /// PRD 6 Test: Maximum capacity (MAX_ADAPTERS_PER_STEP = 8)
    #[test]
    fn test_router_ring_max_capacity() {
        let mut ring = RouterRing::new(MAX_ADAPTERS_PER_STEP);
        let indices: Vec<u16> = (0..8).collect();
        let gates: Vec<i16> = vec![1000; 8];

        let result = ring.set(&indices, &gates);
        assert!(
            result.is_ok(),
            "MAX_ADAPTERS_PER_STEP adapters should be valid"
        );
        assert_eq!(ring.len(), 8);
    }

    /// PRD 6 Test: Duplicate indices (not allowed - violates sorted ascending)
    #[test]
    fn test_router_ring_duplicate_indices() {
        let mut ring = RouterRing::new(4);
        let indices = vec![0, 1, 1, 2]; // Duplicate
        let gates = vec![100, 200, 300, 400];

        let result = ring.set(&indices, &gates);
        assert!(
            result.is_err(),
            "Duplicate indices should fail (not strictly ascending)"
        );
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
