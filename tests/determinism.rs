//! Determinism tests
//!
//! Verify that identical inputs produce identical outputs

use mplora_core::{derive_seed, B3Hash, CPID};
use mplora_manifest::ManifestV3;

#[test]
fn test_hash_deterministic() {
    let data = b"test data";
    let h1 = B3Hash::hash(data);
    let h2 = B3Hash::hash(data);
    assert_eq!(h1, h2, "Hashes must be deterministic");
}

#[test]
fn test_cpid_from_hash_deterministic() {
    let hash = B3Hash::hash(b"test");
    let cpid1 = CPID::from_hash(&hash);
    let cpid2 = CPID::from_hash(&hash);
    assert_eq!(cpid1, cpid2, "CPID derivation must be deterministic");
}

#[test]
fn test_seed_derivation_deterministic() {
    let global = B3Hash::hash(b"global");
    let seed1 = derive_seed(&global, "test");
    let seed2 = derive_seed(&global, "test");
    assert_eq!(seed1, seed2, "Seed derivation must be deterministic");
}
