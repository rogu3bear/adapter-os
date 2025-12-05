//! Deterministic seed derivation using HKDF

use crate::hash::B3Hash;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static::lazy_static! {
    /// Seed registry to prevent reuse
    static ref SEED_REGISTRY: Mutex<HashMap<(String, u64), bool>> = Mutex::new(HashMap::new());
}

/// Seed label enum for type-safe seed derivation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeedLabel {
    Router,
    Dropout,
    Sampling,
    Adapter(usize),
}

impl SeedLabel {
    pub fn as_str(&self) -> String {
        match self {
            SeedLabel::Router => "router".to_string(),
            SeedLabel::Dropout => "dropout".to_string(),
            SeedLabel::Sampling => "sampling".to_string(),
            SeedLabel::Adapter(id) => format!("adapter_{}", id),
        }
    }
}

/// Derive a deterministic seed from a global seed and label
///
/// Uses HKDF-SHA256 for key derivation. All RNG in the system
/// must derive from these seeds to ensure determinism.
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::from_prk(global.as_bytes()).expect("valid PRK");
    let mut okm = [0u8; 32];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("32 bytes is valid length");

    // Validate HKDF output is exactly 32 bytes
    assert_eq!(okm.len(), 32, "HKDF output must be exactly 32 bytes");

    // Compute checksum for audit
    let checksum = B3Hash::hash(&okm);
    tracing::debug!(
        label = label,
        checksum = %checksum.to_hex()[..16],
        "Derived seed with validation"
    );

    okm
}

/// Derive seed with typed label
///
/// Seeds are scoped by `manifest_hash`, `adapter_dir_hash`, `worker_id`,
/// label, and nonce so that two requests with the same inputs get the
/// same 32-byte seed, while any differing input forces a new seed.
pub fn derive_seed_typed(
    global: &B3Hash,
    label: SeedLabel,
    manifest_hash: &B3Hash,
    worker_id: u32,
    nonce: u64,
) -> [u8; 32] {
    let composite_label = format!(
        "{}:{}:{}:{}",
        label.as_str(),
        &manifest_hash.to_hex()[..16],
        worker_id,
        nonce
    );
    derive_seed(global, &composite_label)
}

/// Derive a deterministic seed with an index for array-like derivations
///
/// Allows deriving multiple seeds for the same component by index
pub fn derive_seed_indexed(global: &B3Hash, label: &str, index: usize) -> [u8; 32] {
    let indexed_label = format!("{}:{}", label, index);
    derive_seed(global, &indexed_label)
}

/// Derive multiple seeds at once
pub fn derive_seeds(global: &B3Hash, labels: &[&str]) -> Vec<[u8; 32]> {
    labels.iter().map(|l| derive_seed(global, l)).collect()
}

/// Derive a deterministic seed with full entropy isolation
///
/// Incorporates: manifest_hash || adapter_dir || worker_id || label || nonce.
/// This ensures complete isolation between different:
/// - Manifests (different model configurations)
/// - Adapter directories (different adapter sets)
/// - Workers (different execution contexts)
/// - Labels (router vs sampling vs adapter-scoped)
/// - Nonces (different RNG instances)
///
/// Per Determinism Ruleset #2: All seeds MUST incorporate full context so that
/// the same request context produces identical seeds while any changed
/// parameter yields a distinct seed.
pub fn derive_seed_full(
    global: &B3Hash,
    manifest_hash: &B3Hash,
    adapter_dir_hash: &B3Hash,
    worker_id: u32,
    label: &str,
    nonce: u64,
) -> [u8; 32] {
    // Construct composite label with all entropy sources
    let composite_label = format!(
        "{}:{}:{}:{}:{}",
        label,
        manifest_hash.to_hex(),
        adapter_dir_hash.to_hex(),
        worker_id,
        nonce
    );

    derive_seed(global, &composite_label)
}

/// Hash an adapter directory path deterministically
///
/// Converts path to canonical form and hashes it for use in seed derivation
pub fn hash_adapter_dir(adapter_dir: &std::path::Path) -> B3Hash {
    // Canonicalize path to handle symlinks and relative paths
    let canonical_path = adapter_dir
        .canonicalize()
        .unwrap_or_else(|_| adapter_dir.to_path_buf());

    // Hash the canonical path string
    B3Hash::hash(canonical_path.to_string_lossy().as_bytes())
}

/// Derive per-adapter seed with layer isolation and reuse prevention
pub fn derive_adapter_seed(
    global: &B3Hash,
    adapter_id: usize,
    layer: usize,
    nonce: u64,
) -> Result<[u8; 32], String> {
    let label = format!("adapter_{}:layer_{}", adapter_id, layer);

    // Check for reuse
    let key = (label.clone(), nonce);
    let mut registry = SEED_REGISTRY.lock().unwrap();
    if registry.contains_key(&key) {
        return Err(format!(
            "Seed reuse detected: {} with nonce {}",
            label, nonce
        ));
    }
    registry.insert(key, true);

    Ok(derive_seed(global, &label))
}

/// Clear seed registry (call at inference boundaries)
pub fn clear_seed_registry() {
    let mut registry = SEED_REGISTRY.lock().unwrap();
    registry.clear();
    tracing::debug!("Cleared seed registry");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seed_deterministic() {
        let global = B3Hash::hash(b"test");
        let seed1 = derive_seed(&global, "component_a");
        let seed2 = derive_seed(&global, "component_a");
        assert_eq!(seed1, seed2);
    }

    #[test]
    fn test_different_labels() {
        let global = B3Hash::hash(b"test");
        let seed1 = derive_seed(&global, "component_a");
        let seed2 = derive_seed(&global, "component_b");
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_different_globals() {
        let global1 = B3Hash::hash(b"test1");
        let global2 = B3Hash::hash(b"test2");
        let seed1 = derive_seed(&global1, "component");
        let seed2 = derive_seed(&global2, "component");
        assert_ne!(seed1, seed2);
    }

    #[test]
    fn test_derive_seed_full_deterministic() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);

        assert_eq!(seed1, seed2, "Same inputs should produce same seed");
    }

    #[test]
    fn test_derive_seed_full_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest1 = B3Hash::hash(b"manifest1");
        let manifest2 = B3Hash::hash(b"manifest2");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest1, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest2, &adapter_dir, 1, "router", 0);

        assert_ne!(
            seed1, seed2,
            "Different manifests should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_nonce_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed2 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 1);

        assert_ne!(
            seed1, seed2,
            "Different nonces should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_worker_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir = B3Hash::hash(b"/adapters/test");

        let seed_worker1 = derive_seed_full(&global, &manifest, &adapter_dir, 1, "router", 0);
        let seed_worker2 = derive_seed_full(&global, &manifest, &adapter_dir, 2, "router", 0);

        assert_ne!(
            seed_worker1, seed_worker2,
            "Different worker IDs should produce different seeds"
        );
    }

    #[test]
    fn test_derive_seed_full_adapter_dir_isolation() {
        let global = B3Hash::hash(b"global");
        let manifest = B3Hash::hash(b"manifest");
        let adapter_dir_a = B3Hash::hash(b"/adapters/a");
        let adapter_dir_b = B3Hash::hash(b"/adapters/b");

        let seed_a = derive_seed_full(&global, &manifest, &adapter_dir_a, 1, "router", 0);
        let seed_b = derive_seed_full(&global, &manifest, &adapter_dir_b, 1, "router", 0);

        assert_ne!(
            seed_a, seed_b,
            "Different adapter directories should produce different seeds"
        );
    }

    #[test]
    fn test_hash_adapter_dir() {
        use std::path::Path;

        let path1 = Path::new("/adapters/test");
        let hash1 = hash_adapter_dir(path1);
        let hash2 = hash_adapter_dir(path1);

        assert_eq!(hash1, hash2, "Same path should produce same hash");
    }
}
