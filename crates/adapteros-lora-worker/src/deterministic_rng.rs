//! Deterministic RNG implementation using HKDF
//!
//! Per AdapterOS Policy Ruleset #2 (Determinism):
//! - MUST derive all RNG from `seed_global` and HKDF labels
//! - MUST ensure identical inputs produce identical outputs
//! - MUST record toolchain version strings and kernel hashes in Plan metadata

use hkdf::Hkdf;
use adapteros_core::{AosError, Result};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use sha2::Sha256;

/// Deterministic RNG derived from global seed using HKDF
pub struct DeterministicRng {
    /// The underlying StdRng seeded deterministically
    rng: StdRng,
    /// Label used for HKDF derivation
    label: String,
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
        // Use HKDF to derive a domain-specific seed from the global seed
        let hk = Hkdf::<Sha256>::new(None, seed_global);
        let mut derived_seed = [0u8; 32];

        hk.expand(label.as_bytes(), &mut derived_seed)
            .map_err(|e| AosError::Other(format!("HKDF expansion failed: {}", e)))?;

        // Seed StdRng with the derived seed
        let rng = StdRng::from_seed(derived_seed);

        tracing::debug!(
            label = label,
            seed_hash = hex::encode(&derived_seed[..8]),
            "Initialized deterministic RNG"
        );

        Ok(Self {
            rng,
            label: label.to_string(),
        })
    }

    /// Get the label for this RNG
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Generate a random u64
    pub fn next_u64(&mut self) -> u64 {
        self.rng.next_u64()
    }

    /// Generate a random u32
    pub fn next_u32(&mut self) -> u32 {
        self.rng.next_u32()
    }

    /// Generate a random f32 in [0.0, 1.0)
    pub fn next_f32(&mut self) -> f32 {
        self.rng.next_u32() as f32 / (u32::MAX as f32 + 1.0)
    }

    /// Generate a random f64 in [0.0, 1.0)
    pub fn next_f64(&mut self) -> f64 {
        self.rng.next_u64() as f64 / (u64::MAX as f64 + 1.0)
    }

    /// Fill a buffer with random bytes
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.rng.fill_bytes(dest);
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

/// RNG factory for creating domain-specific RNGs
pub struct RngFactory {
    seed_global: [u8; 32],
}

impl RngFactory {
    /// Create a new RNG factory from global seed
    pub fn new(seed_global: [u8; 32]) -> Self {
        Self { seed_global }
    }

    /// Create an RNG for router operations
    pub fn router_rng(&self) -> Result<DeterministicRng> {
        DeterministicRng::new(&self.seed_global, "router")
    }

    /// Create an RNG for dropout operations
    pub fn dropout_rng(&self) -> Result<DeterministicRng> {
        DeterministicRng::new(&self.seed_global, "dropout")
    }

    /// Create an RNG for sampling operations
    pub fn sampling_rng(&self) -> Result<DeterministicRng> {
        DeterministicRng::new(&self.seed_global, "sampling")
    }

    /// Create an RNG with custom label
    pub fn custom_rng(&self, label: &str) -> Result<DeterministicRng> {
        DeterministicRng::new(&self.seed_global, label)
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
        let factory = RngFactory::new(seed);

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

        // Same type should produce same sequence
        assert_eq!(router_rng1.next_u64(), router_rng2.next_u64());

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
}
