//! Deterministic RNG implementation using HKDF
//!
//! Per adapterOS Policy Ruleset #2 (Determinism):
//! - MUST derive all RNG from `seed_global` and HKDF labels
//! - MUST ensure identical inputs produce identical outputs
//! - MUST record toolchain version strings and kernel hashes in Plan metadata
#![allow(clippy::manual_range_contains)]

use adapteros_core::{derive_seed, B3Hash, Result};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zeroize::Zeroize;

/// Global nonce counter for ensuring unique RNG instances
static NEXT_NONCE: AtomicU64 = AtomicU64::new(0);

/// Deterministic RNG derived from global seed using HKDF
pub struct DeterministicRng {
    /// The underlying ChaCha20Rng seeded deterministically
    rng: ChaCha20Rng,
    /// Label used for HKDF derivation
    label: String,
    /// Original seed for re-initialization
    seed: [u8; 32],
    /// Step counter for state tracking
    step_count: u64,
}

impl DeterministicRng {
    /// Create a new deterministic RNG from global seed and label
    ///
    /// # Arguments
    /// * `seed_global` - The global seed (32 bytes)
    /// * `label` - HKDF label for domain separation (e.g., "router", "dropout", "sampling")
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_lora_worker::deterministic_rng::DeterministicRng;
    ///
    /// let global_seed = [0u8; 32]; // From manifest
    /// let rng = DeterministicRng::new(&global_seed, "router").unwrap();
    /// ```
    pub fn new(seed_global: &[u8; 32], label: &str) -> Result<Self> {
        // Use canonical HKDF derivation from the global seed.
        let derived_seed = derive_seed(&B3Hash::new(*seed_global), label);

        // Compute checksum for audit
        let checksum = B3Hash::hash(&derived_seed);

        // Seed ChaCha20Rng with the derived seed
        let rng = ChaCha20Rng::from_seed(derived_seed);

        tracing::debug!(
            label = label,
            global_seed = %hex::encode(&seed_global[..8]),
            derived_seed = %hex::encode(&derived_seed[..8]),
            checksum = %checksum.to_hex()[..16],
            "Initialized deterministic RNG with validation"
        );

        Ok(Self {
            rng,
            label: label.to_string(),
            seed: derived_seed,
            step_count: 0,
        })
    }

    /// Get the label for this RNG
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Generate a random u64
    #[inline(never)]
    pub fn next_u64(&mut self) -> u64 {
        self.step_count += 1;
        self.rng.next_u64()
    }

    /// Generate a random u32
    #[inline(never)]
    pub fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    /// Generate a random f32 in [0.0, 1.0)
    #[inline(never)]
    pub fn next_f32(&mut self) -> f32 {
        std::hint::black_box(self.rng.next_u32()) as f32 / (u32::MAX as f32 + 1.0)
    }

    /// Generate a random f64 in [0.0, 1.0)
    #[inline(never)]
    pub fn next_f64(&mut self) -> f64 {
        std::hint::black_box(self.rng.next_u64()) as f64 / (u64::MAX as f64 + 1.0)
    }

    /// Fill a buffer with random bytes
    #[inline(never)]
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(std::hint::black_box(dest));
    }

    /// Generate a random value in range [0, n)
    pub fn gen_range_u32(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        // Use rejection sampling for uniform distribution
        let threshold = (u32::MAX - n + 1) % n;
        loop {
            let val = self.next_u32();
            if val >= threshold {
                return val % n;
            }
        }
    }
}

impl RngCore for DeterministicRng {
    fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), rand::Error> {
        self.rng.try_fill_bytes(dest)
    }
}

/// RNG state for serialization/replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RngState {
    /// The seed used to initialize this RNG
    pub seed: [u8; 32],
    /// The label for this RNG
    pub label: String,
    /// Number of steps taken
    pub step_count: u64,
    /// Current nonce value
    pub nonce: u64,
}

impl DeterministicRng {
    /// Serialize RNG state for replay
    pub fn serialize_state(&self) -> RngState {
        RngState {
            seed: [0u8; 32], // Will be reconstructed from context
            label: self.label.clone(),
            step_count: self.step_count,
            nonce: get_global_nonce(),
        }
    }

    /// Restore RNG from serialized state
    pub fn restore_state(state: &RngState, seed: &[u8; 32]) -> Result<Self> {
        let mut rng = Self::new(seed, &state.label)?;

        // Fast-forward to the correct state by consuming steps
        for _ in 0..state.step_count {
            rng.rng.next_u64();
        }
        rng.step_count = state.step_count;

        Ok(rng)
    }

    /// Get the current step count
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    /// Create a checkpoint at a specific phase
    pub fn checkpoint(&self, phase: &str, tick: u64) -> RngCheckpoint {
        RngCheckpoint {
            timestamp_ticks: tick,
            phase: phase.to_string(),
            state: self.serialize_state(),
        }
    }
}

/// RNG checkpoint for mid-inference state capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RngCheckpoint {
    pub timestamp_ticks: u64,
    pub phase: String,
    pub state: RngState,
}

/// Implement Drop to securely zero RNG state
impl Drop for DeterministicRng {
    fn drop(&mut self) {
        // Zero out the seed
        self.seed.zeroize();
        tracing::trace!(label = %self.label, "Zeroed RNG state on drop");
    }
}

/// Get the global nonce counter value
pub fn get_global_nonce() -> u64 {
    NEXT_NONCE.load(Ordering::SeqCst)
}

/// Set the global nonce counter value (for replay)
pub fn set_global_nonce(n: u64) {
    NEXT_NONCE.store(n, Ordering::SeqCst);
}

/// RNG factory for creating domain-specific RNGs with full entropy isolation
pub struct RngFactory {
    seed_global: [u8; 32],
    manifest_hash: B3Hash,
    adapter_dir: PathBuf,
    worker_id: u32,
}

impl RngFactory {
    /// Create a new RNG factory with full context
    ///
    /// Per Determinism Ruleset #2: All RNG must incorporate
    /// manifest_hash + adapter_dir + worker_id + nonce
    pub fn new(
        seed_global: [u8; 32],
        manifest_hash: B3Hash,
        adapter_dir: PathBuf,
        worker_id: u32,
    ) -> Self {
        Self {
            seed_global,
            manifest_hash,
            adapter_dir,
            worker_id,
        }
    }

    /// Create from global seed only (for compatibility)
    pub fn from_global_seed(seed_global: [u8; 32], worker_id: u32) -> Self {
        Self {
            seed_global,
            manifest_hash: B3Hash::hash(b"default_manifest"),
            adapter_dir: PathBuf::from("/adapters/default"),
            worker_id,
        }
    }

    /// Create an RNG for router operations with full entropy
    pub fn router_rng(&self) -> Result<DeterministicRng> {
        let n = NEXT_NONCE.fetch_add(1, Ordering::SeqCst);
        let adapter_dir_hash = adapteros_core::hash_adapter_dir(&self.adapter_dir);
        let seed = adapteros_core::derive_seed_full(
            &B3Hash::new(self.seed_global),
            &self.manifest_hash,
            &adapter_dir_hash,
            self.worker_id,
            "router",
            n,
        );
        DeterministicRng::new(
            &seed,
            &format!(
                "router:{}:{}:{}",
                &self.manifest_hash.to_hex()[..8],
                self.worker_id,
                n
            ),
        )
    }

    /// Create an RNG for dropout operations with full entropy
    pub fn dropout_rng(&self) -> Result<DeterministicRng> {
        let n = NEXT_NONCE.fetch_add(1, Ordering::SeqCst);
        let adapter_dir_hash = adapteros_core::hash_adapter_dir(&self.adapter_dir);
        let seed = adapteros_core::derive_seed_full(
            &B3Hash::new(self.seed_global),
            &self.manifest_hash,
            &adapter_dir_hash,
            self.worker_id,
            "dropout",
            n,
        );
        DeterministicRng::new(
            &seed,
            &format!(
                "dropout:{}:{}:{}",
                &self.manifest_hash.to_hex()[..8],
                self.worker_id,
                n
            ),
        )
    }

    /// Create an RNG for sampling operations with full entropy
    pub fn sampling_rng(&self) -> Result<DeterministicRng> {
        let n = NEXT_NONCE.fetch_add(1, Ordering::SeqCst);
        let adapter_dir_hash = adapteros_core::hash_adapter_dir(&self.adapter_dir);
        let seed = adapteros_core::derive_seed_full(
            &B3Hash::new(self.seed_global),
            &self.manifest_hash,
            &adapter_dir_hash,
            self.worker_id,
            "sampling",
            n,
        );
        DeterministicRng::new(
            &seed,
            &format!(
                "sampling:{}:{}:{}",
                &self.manifest_hash.to_hex()[..8],
                self.worker_id,
                n
            ),
        )
    }

    /// Create an RNG with custom label and full entropy
    pub fn custom_rng(&self, label: &str) -> Result<DeterministicRng> {
        let n = NEXT_NONCE.fetch_add(1, Ordering::SeqCst);
        let adapter_dir_hash = adapteros_core::hash_adapter_dir(&self.adapter_dir);
        let seed = adapteros_core::derive_seed_full(
            &B3Hash::new(self.seed_global),
            &self.manifest_hash,
            &adapter_dir_hash,
            self.worker_id,
            label,
            n,
        );
        DeterministicRng::new(
            &seed,
            &format!(
                "{}:{}:{}:{}",
                label,
                &self.manifest_hash.to_hex()[..8],
                self.worker_id,
                n
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_rng_reproducibility() {
        let seed = [42u8; 32];

        // Create two RNGs with the same seed and label
        let mut rng1 =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");
        let mut rng2 =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");

        // They should produce identical sequences
        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_deterministic_rng_different_labels() {
        let seed = [42u8; 32];

        // Create two RNGs with the same seed but different labels
        let mut rng1 =
            DeterministicRng::new(&seed, "label1").expect("Test RNG creation should succeed");
        let mut rng2 =
            DeterministicRng::new(&seed, "label2").expect("Test RNG creation should succeed");

        // They should produce different sequences
        let val1 = rng1.next_u64();
        let val2 = rng2.next_u64();
        assert_ne!(val1, val2);
    }

    #[test]
    fn test_deterministic_rng_different_seeds() {
        let seed1 = [42u8; 32];
        let seed2 = [43u8; 32];

        // Create two RNGs with different seeds
        let mut rng1 =
            DeterministicRng::new(&seed1, "test").expect("Test RNG creation should succeed");
        let mut rng2 =
            DeterministicRng::new(&seed2, "test").expect("Test RNG creation should succeed");

        // They should produce different sequences
        let val1 = rng1.next_u64();
        let val2 = rng2.next_u64();
        assert_ne!(val1, val2);
    }

    #[test]
    fn test_rng_factory() {
        let seed = [42u8; 32];
        let factory = RngFactory::from_global_seed(seed, 1);

        // Create different RNGs from factory
        let mut router_rng1 = factory
            .router_rng()
            .expect("Test RNG creation should succeed");
        let mut router_rng2 = factory
            .router_rng()
            .expect("Test RNG creation should succeed");
        let mut dropout_rng = factory
            .dropout_rng()
            .expect("Test RNG creation should succeed");

        // Different instances should produce different sequences due to nonce
        assert_ne!(router_rng1.next_u64(), router_rng2.next_u64());

        // Different types should produce different sequences
        let router_val = router_rng1.next_u64();
        let dropout_val = dropout_rng.next_u64();
        assert_ne!(router_val, dropout_val);
    }

    #[test]
    fn test_gen_range_uniform() {
        let seed = [42u8; 32];
        let mut rng =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");

        // Generate many values and check they're in range
        for _ in 0..1000 {
            let val = rng.gen_range_u32(10);
            assert!(val < 10);
        }
    }

    #[test]
    fn test_f32_range() {
        let seed = [42u8; 32];
        let mut rng =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");

        // Generate many values and check they're in [0.0, 1.0)
        for _ in 0..1000 {
            let val = rng.next_f32();
            assert!(val >= 0.0 && val < 1.0);
        }
    }

    #[test]
    fn test_fill_bytes_deterministic() {
        let seed = [42u8; 32];

        let mut rng1 =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");
        let mut rng2 =
            DeterministicRng::new(&seed, "test").expect("Test RNG creation should succeed");

        let mut buf1 = [0u8; 100];
        let mut buf2 = [0u8; 100];

        rng1.fill_bytes(&mut buf1);
        rng2.fill_bytes(&mut buf2);

        assert_eq!(buf1, buf2);
    }

    #[test]
    fn test_rng_state_serialization() {
        let seed = [42u8; 32];
        let mut rng = DeterministicRng::new(&seed, "test").expect("RNG creation should succeed");

        // Generate some random values
        for _ in 0..10 {
            rng.next_u64();
        }

        // Serialize state
        let state = rng.serialize_state();
        assert_eq!(state.step_count, 10);
        assert_eq!(state.label, "test");
    }

    #[test]
    fn test_rng_state_restoration() {
        let seed = [42u8; 32];
        let mut rng1 = DeterministicRng::new(&seed, "test").expect("RNG creation should succeed");

        // Generate values and serialize
        let mut values1 = Vec::new();
        for _ in 0..10 {
            values1.push(rng1.next_u64());
        }
        let state = rng1.serialize_state();

        // Restore and continue
        let mut rng2 =
            DeterministicRng::restore_state(&state, &seed).expect("Restoration should succeed");
        let next1 = rng1.next_u64();
        let next2 = rng2.next_u64();

        assert_eq!(next1, next2, "Restored RNG should continue from same state");
    }

    #[test]
    fn test_global_nonce_persistence() {
        let initial = get_global_nonce();
        set_global_nonce(12345);
        assert_eq!(get_global_nonce(), 12345);
        set_global_nonce(initial); // Restore for other tests
    }
}
