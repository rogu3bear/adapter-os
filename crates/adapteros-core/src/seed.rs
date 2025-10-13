//! Deterministic seed derivation using HKDF

use crate::hash::B3Hash;
use hkdf::Hkdf;
use sha2::Sha256;

/// Derive a deterministic seed from a global seed and label
///
/// Uses HKDF-SHA256 for key derivation. All RNG in the system
/// must derive from these seeds to ensure determinism.
pub fn derive_seed(global: &B3Hash, label: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::from_prk(global.as_bytes()).expect("valid PRK");
    let mut okm = [0u8; 32];
    hk.expand(label.as_bytes(), &mut okm)
        .expect("32 bytes is valid length");
    okm
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
}
