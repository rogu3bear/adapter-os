//! Determinism policy enforcement using HKDF-SHA256 seeding
//!
//! This module provides deterministic seed derivation for all RNG operations
//! in the AdapterOS system. Per Policy Ruleset #2 (Determinism), all randomness
//! must be derived from a global seed using HKDF with domain separation labels.
//!
//! # Supported Domains
//!
//! - `Router` - Adapter selection and routing decisions
//! - `Dropout` - Dropout masks during training/inference
//! - `Sampling` - Token sampling during generation
//! - `Adapter(n)` - Per-adapter operations
//! - `Custom` - User-defined operations
//!
//! # Example
//!
//! ```rust,no_run
//! use adapteros_lora_worker::services::determinism_policy::{HkdfSeedExpander, SeedDomain};
//!
//! // Create expander from manifest seed
//! let seed = [0u8; 32]; // Typically from manifest.seeds.global
//! let mut expander = HkdfSeedExpander::new(&seed);
//!
//! // Derive domain-specific seeds
//! let router_seed = expander.derive(SeedDomain::Router);
//! let sampling_seed = expander.derive(SeedDomain::Sampling);
//!
//! // Create seeded RNG for sampling
//! let mut rng = expander.create_rng(SeedDomain::Sampling);
//! ```

use adapteros_core::{derive_seed, derive_seed_full, derive_seed_indexed, hash_adapter_dir};
use adapteros_core::{AosError, B3Hash, Result};
use hkdf::Hkdf;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use sha2::Sha256;
use std::path::Path;

/// Domain labels for HKDF seed derivation
/// Per AdapterOS Policy Ruleset #2 (Determinism)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedDomain {
    /// Router operations (adapter selection)
    Router,
    /// Dropout during training/inference
    Dropout,
    /// Token sampling during generation
    Sampling,
    /// Adapter-specific operations
    Adapter(usize),
    /// Custom domain with explicit label
    Custom,
}

impl SeedDomain {
    /// Convert domain to HKDF label string
    pub fn as_label(&self) -> String {
        match self {
            SeedDomain::Router => "router".to_string(),
            SeedDomain::Dropout => "dropout".to_string(),
            SeedDomain::Sampling => "sampling".to_string(),
            SeedDomain::Adapter(id) => format!("adapter_{}", id),
            SeedDomain::Custom => "custom".to_string(),
        }
    }
}

/// HKDF-based deterministic seed expander
///
/// Provides domain-separated seed derivation using HKDF-SHA256.
/// All RNG in the system must derive from seeds produced by this expander
/// to ensure reproducible execution.
pub struct HkdfSeedExpander {
    /// Base seed (32 bytes, typically from manifest hash)
    base_seed: B3Hash,
    /// Optional manifest hash for full entropy isolation
    manifest_hash: Option<B3Hash>,
    /// Optional adapter directory hash
    adapter_dir_hash: Option<B3Hash>,
    /// Worker ID for multi-worker isolation
    worker_id: u32,
    /// Nonce counter for unique seed instances
    nonce: u64,
}

impl HkdfSeedExpander {
    /// Create a new HKDF seed expander from a base seed
    ///
    /// # Arguments
    /// * `seed` - Base seed bytes (32 bytes recommended, will be hashed if different)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_lora_worker::services::determinism_policy::HkdfSeedExpander;
    ///
    /// let seed = [0u8; 32]; // From manifest hash
    /// let expander = HkdfSeedExpander::new(&seed);
    /// ```
    pub fn new(seed: &[u8]) -> Self {
        let base_seed = if seed.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(seed);
            B3Hash::new(arr)
        } else {
            B3Hash::hash(seed)
        };

        Self {
            base_seed,
            manifest_hash: None,
            adapter_dir_hash: None,
            worker_id: 0,
            nonce: 0,
        }
    }

    /// Create expander with full entropy isolation
    ///
    /// Per Determinism Ruleset #2: All seeds should incorporate
    /// manifest_hash + adapter_dir + worker_id for complete isolation
    pub fn with_full_context(
        seed: &[u8],
        manifest_hash: &B3Hash,
        adapter_dir: &Path,
        worker_id: u32,
    ) -> Self {
        let base_seed = if seed.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(seed);
            B3Hash::new(arr)
        } else {
            B3Hash::hash(seed)
        };

        Self {
            base_seed,
            manifest_hash: Some(*manifest_hash),
            adapter_dir_hash: Some(hash_adapter_dir(adapter_dir)),
            worker_id,
            nonce: 0,
        }
    }

    /// Derive a deterministic seed for the specified domain
    ///
    /// Uses HKDF-SHA256 for key derivation with domain separation
    pub fn derive(&mut self, domain: SeedDomain) -> [u8; 32] {
        let label = domain.as_label();
        self.derive_with_label(&label)
    }

    /// Derive a deterministic seed with a custom label
    pub fn derive_with_label(&mut self, label: &str) -> [u8; 32] {
        let seed = if let (Some(manifest), Some(adapter_dir)) =
            (&self.manifest_hash, &self.adapter_dir_hash)
        {
            // Full entropy isolation
            let nonce = self.nonce;
            self.nonce += 1;
            derive_seed_full(
                &self.base_seed,
                manifest,
                adapter_dir,
                self.worker_id,
                label,
                nonce,
            )
        } else {
            // Simple derivation
            derive_seed(&self.base_seed, label)
        };

        // Compute checksum for audit trail
        let checksum = B3Hash::hash(&seed);
        tracing::debug!(
            label = label,
            checksum = %checksum.to_hex()[..16],
            worker_id = self.worker_id,
            "Derived HKDF seed"
        );

        seed
    }

    /// Derive an indexed seed for array-like operations
    ///
    /// Useful for deriving multiple seeds for the same component
    pub fn derive_indexed(&self, label: &str, index: usize) -> [u8; 32] {
        derive_seed_indexed(&self.base_seed, label, index)
    }

    /// Create a ChaCha20Rng seeded from this expander
    pub fn create_rng(&mut self, domain: SeedDomain) -> ChaCha20Rng {
        let seed = self.derive(domain);
        ChaCha20Rng::from_seed(seed)
    }

    /// Create a ChaCha20Rng with custom label
    pub fn create_rng_with_label(&mut self, label: &str) -> ChaCha20Rng {
        let seed = self.derive_with_label(label);
        ChaCha20Rng::from_seed(seed)
    }

    /// Get the base seed hash (for audit/logging)
    pub fn base_seed_hash(&self) -> &B3Hash {
        &self.base_seed
    }

    /// Get current nonce value
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
}

/// Legacy wrapper for backward compatibility
///
/// Creates an HKDF expander and derives seeds for seeded operations
pub fn seed_rng_hkdf(seed: &[u8]) -> Result<HkdfSeedExpander> {
    Ok(HkdfSeedExpander::new(seed))
}

/// Derive a domain-specific seed using HKDF-SHA256
///
/// # Arguments
/// * `global_seed` - The global seed (32 bytes)
/// * `domain` - The domain for seed separation
///
/// # Returns
/// A 32-byte deterministic seed
pub fn derive_domain_seed(global_seed: &[u8; 32], domain: SeedDomain) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha256>::new(None, global_seed);
    let mut okm = [0u8; 32];
    let label = domain.as_label();

    hk.expand(label.as_bytes(), &mut okm).map_err(|e| {
        AosError::Crypto(format!(
            "HKDF expansion failed for domain '{}': {}",
            label, e
        ))
    })?;

    Ok(okm)
}

/// Derive multiple domain seeds at once
pub fn derive_domain_seeds(
    global_seed: &[u8; 32],
    domains: &[SeedDomain],
) -> Result<Vec<[u8; 32]>> {
    domains
        .iter()
        .map(|d| derive_domain_seed(global_seed, *d))
        .collect()
}

/// Validate backend attestation using BLAKE3 hash comparison
///
/// This function is used to validate that the backend output matches
/// the expected hash for the given backend type.
pub fn validate_backend_attestation(backend: &str, output: &[u8]) -> Result<()> {
    let expected_hash = B3Hash::hash(output);
    let backend_hash = match backend {
        "metal" => B3Hash::hash(b"metal"),
        "mlx" => B3Hash::hash(b"mlx"),
        "coreml" => B3Hash::hash(b"coreml"),
        _ => {
            return Err(AosError::DeterminismViolation(format!(
                "Unknown backend: {}",
                backend
            )))
        }
    };
    if expected_hash != backend_hash {
        return Err(AosError::DeterminismViolation(format!(
            "Attestation failed: expected {} but got {}",
            &backend_hash.to_hex()[..16],
            &expected_hash.to_hex()[..16]
        )));
    }
    Ok(())
}

/// Policy enforcement for deterministic execution
///
/// Validates that the execution is deterministic by:
/// 1. Deriving seeds from input using HKDF
/// 2. Validating backend attestation
///
/// # Arguments
/// * `input` - Input data to derive seed from
/// * `output` - Output data to validate attestation against
/// * `backend` - Optional backend name (defaults to "metal")
pub fn enforce_determinism_policy(input: &[u8], output: &[u8]) -> Result<()> {
    enforce_determinism_policy_with_backend(input, output, "metal")
}

/// Policy enforcement with explicit backend selection
pub fn enforce_determinism_policy_with_backend(
    input: &[u8],
    output: &[u8],
    backend: &str,
) -> Result<()> {
    let seed = blake3::hash(input);
    let mut expander = seed_rng_hkdf(seed.as_bytes())?;

    // Derive seeds for standard domains to ensure they're available
    let _router_seed = expander.derive(SeedDomain::Router);
    let _dropout_seed = expander.derive(SeedDomain::Dropout);
    let _sampling_seed = expander.derive(SeedDomain::Sampling);

    tracing::debug!(
        base_seed = %expander.base_seed_hash().to_hex()[..16],
        backend = backend,
        "Enforcing determinism policy"
    );

    validate_backend_attestation(backend, output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[test]
    fn test_seed_domain_labels() {
        assert_eq!(SeedDomain::Router.as_label(), "router");
        assert_eq!(SeedDomain::Dropout.as_label(), "dropout");
        assert_eq!(SeedDomain::Sampling.as_label(), "sampling");
        assert_eq!(SeedDomain::Adapter(5).as_label(), "adapter_5");
        assert_eq!(SeedDomain::Custom.as_label(), "custom");
    }

    #[test]
    fn test_hkdf_expander_deterministic() {
        let seed = [42u8; 32];

        let mut exp1 = HkdfSeedExpander::new(&seed);
        let mut exp2 = HkdfSeedExpander::new(&seed);

        // Same domain should produce same seed
        let s1 = exp1.derive(SeedDomain::Router);
        let s2 = exp2.derive(SeedDomain::Router);
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_hkdf_expander_domain_separation() {
        let seed = [42u8; 32];
        let mut expander = HkdfSeedExpander::new(&seed);

        let router_seed = expander.derive(SeedDomain::Router);
        let dropout_seed = expander.derive(SeedDomain::Dropout);
        let sampling_seed = expander.derive(SeedDomain::Sampling);

        // Different domains should produce different seeds
        assert_ne!(router_seed, dropout_seed);
        assert_ne!(router_seed, sampling_seed);
        assert_ne!(dropout_seed, sampling_seed);
    }

    #[test]
    fn test_hkdf_expander_adapter_separation() {
        let seed = [42u8; 32];
        let mut expander = HkdfSeedExpander::new(&seed);

        let adapter0 = expander.derive(SeedDomain::Adapter(0));
        let adapter1 = expander.derive(SeedDomain::Adapter(1));
        let adapter2 = expander.derive(SeedDomain::Adapter(2));

        // Different adapters should produce different seeds
        assert_ne!(adapter0, adapter1);
        assert_ne!(adapter1, adapter2);
    }

    #[test]
    fn test_hkdf_expander_indexed() {
        let seed = [42u8; 32];
        let expander = HkdfSeedExpander::new(&seed);

        let idx0 = expander.derive_indexed("layer", 0);
        let idx1 = expander.derive_indexed("layer", 1);
        let idx2 = expander.derive_indexed("layer", 2);

        // Same index should be deterministic
        let idx0_again = expander.derive_indexed("layer", 0);
        assert_eq!(idx0, idx0_again);

        // Different indices should produce different seeds
        assert_ne!(idx0, idx1);
        assert_ne!(idx1, idx2);
    }

    #[test]
    fn test_derive_domain_seed_deterministic() {
        let seed = [42u8; 32];

        let s1 = derive_domain_seed(&seed, SeedDomain::Router).unwrap();
        let s2 = derive_domain_seed(&seed, SeedDomain::Router).unwrap();

        assert_eq!(s1, s2);
    }

    #[test]
    fn test_derive_domain_seeds_batch() {
        let seed = [42u8; 32];
        let domains = [
            SeedDomain::Router,
            SeedDomain::Dropout,
            SeedDomain::Sampling,
        ];

        let seeds = derive_domain_seeds(&seed, &domains).unwrap();

        assert_eq!(seeds.len(), 3);
        // All seeds should be different
        assert_ne!(seeds[0], seeds[1]);
        assert_ne!(seeds[1], seeds[2]);
    }

    #[test]
    fn test_hkdf_expander_create_rng() {
        let seed = [42u8; 32];

        let mut exp1 = HkdfSeedExpander::new(&seed);
        let mut exp2 = HkdfSeedExpander::new(&seed);

        let mut rng1 = exp1.create_rng(SeedDomain::Sampling);
        let mut rng2 = exp2.create_rng(SeedDomain::Sampling);

        // RNGs should produce identical sequences
        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_different_base_seeds() {
        let seed1 = [1u8; 32];
        let seed2 = [2u8; 32];

        let mut exp1 = HkdfSeedExpander::new(&seed1);
        let mut exp2 = HkdfSeedExpander::new(&seed2);

        let s1 = exp1.derive(SeedDomain::Router);
        let s2 = exp2.derive(SeedDomain::Router);

        // Different base seeds should produce different derived seeds
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_seed_rng_hkdf_compatibility() {
        let seed = [42u8; 32];

        let mut expander = seed_rng_hkdf(&seed).unwrap();
        let derived = expander.derive(SeedDomain::Router);

        // Should produce valid 32-byte seed
        assert_eq!(derived.len(), 32);
    }

    #[test]
    fn test_variable_length_seed() {
        // Short seed should be hashed
        let short_seed = [1u8; 16];
        let exp1 = HkdfSeedExpander::new(&short_seed);

        // Long seed should be hashed
        let long_seed = [2u8; 64];
        let exp2 = HkdfSeedExpander::new(&long_seed);

        // Both should work and produce different results
        assert_ne!(exp1.base_seed_hash(), exp2.base_seed_hash());
    }
}
